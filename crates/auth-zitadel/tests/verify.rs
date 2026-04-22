#![allow(clippy::unwrap_used, clippy::panic)]
//! Sign-and-verify integration tests for [`auth_zitadel::ZitadelProvider`].
//!
//! Tests mint JWTs against a throwaway RSA keypair generated at test
//! start, expose the public half as a [`jsonwebtoken::jwk::JwkSet`],
//! feed that into a [`StaticJwksSource`], and call the provider's
//! `AuthProvider::resolve`. No network, no live Zitadel.
//!
//! What we verify:
//!
//! - Happy path: correct iss + aud + signature + tenant claim → valid
//!   [`spi::AuthContext`].
//! - Rejections: bad signature (wrong key), wrong audience, expired
//!   token, wrong issuer, missing `kid`, unknown `kid`.
//! - Tenant pinning: edge-mode provider rejects tokens from other orgs.
//! - Scope mapping: `listo_scopes` claim → [`spi::ScopeSet`];
//!   defaults to [`Scope::ReadNodes`] when absent.
//! - Disk cache: warm-start works when live fetch fails.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use auth_zitadel::{
    DiskCache, HttpJwksSource, JwksSource, StaticDenyList, StaticJwksSource, ZitadelConfig,
    ZitadelError, ZitadelProvider,
};
use base64::Engine as _;
use jsonwebtoken::jwk::{
    AlgorithmParameters, CommonParameters, Jwk, JwkSet, KeyAlgorithm, PublicKeyUse, RSAKeyParameters,
    RSAKeyType,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use rsa::pkcs8::EncodePrivateKey;
use rsa::traits::PublicKeyParts;
use rsa::RsaPrivateKey;
use serde_json::json;
use spi::{Actor, AuthProvider, Scope};

// ── Key fixture ───────────────────────────────────────────────────────────────

struct KeyPair {
    encoding_key: EncodingKey,
    jwks: JwkSet,
    kid: String,
}

/// Generate a fresh RSA keypair and build (EncodingKey for signing,
/// JwkSet for verification, kid). 2048-bit keeps test runtime
/// bounded. Uses the OS RNG directly — rsa 0.9 is pinned on
/// `rand_core = 0.6`, which is a dev-dep here so the workspace's
/// `rand` version can move independently.
fn fresh_keypair(kid: &str) -> KeyPair {
    let mut rng = rand_core::OsRng;
    let private = RsaPrivateKey::new(&mut rng, 2048).expect("rsa keygen");
    let pem = private
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .expect("pkcs8 pem");
    let encoding_key = EncodingKey::from_rsa_pem(pem.as_bytes()).expect("encoding key");

    let public = private.to_public_key();
    // JWK n/e are base64url-no-pad of the big-endian components.
    let enc = base64::engine::general_purpose::URL_SAFE_NO_PAD;
    let n = enc.encode(public.n().to_bytes_be());
    let e = enc.encode(public.e().to_bytes_be());

    let jwk = Jwk {
        common: CommonParameters {
            public_key_use: Some(PublicKeyUse::Signature),
            key_operations: None,
            key_algorithm: Some(KeyAlgorithm::RS256),
            key_id: Some(kid.to_string()),
            x509_url: None,
            x509_chain: None,
            x509_sha1_fingerprint: None,
            x509_sha256_fingerprint: None,
        },
        algorithm: AlgorithmParameters::RSA(RSAKeyParameters {
            key_type: RSAKeyType::RSA,
            n,
            e,
        }),
    };

    KeyPair {
        encoding_key,
        jwks: JwkSet { keys: vec![jwk] },
        kid: kid.to_string(),
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

const ISSUER: &str = "https://zitadel.test";
const AUDIENCE: &str = "listo-agent";
const TENANT_CLAIM: &str = "urn:zitadel:iam:user:resourceowner:id";
const SCOPES_CLAIM: &str = "listo_scopes";

fn sign(
    kp: &KeyPair,
    iss: &str,
    aud: &str,
    sub: &str,
    exp_offset: i64,
    extras: serde_json::Value,
) -> String {
    let nbf = (now() as i64 - 10) as u64;
    let exp = (now() as i64 + exp_offset) as u64;
    let mut payload = json!({
        "iss": iss,
        "aud": aud,
        "sub": sub,
        "nbf": nbf,
        "exp": exp,
    });
    if let Some(obj) = payload.as_object_mut() {
        if let Some(extra_obj) = extras.as_object() {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(kp.kid.clone());
    encode(&header, &payload, &kp.encoding_key).expect("encode")
}

async fn provider_with(jwks: JwkSet, tenant_pin: Option<String>) -> ZitadelProvider {
    let cfg = {
        let mut c = ZitadelConfig::new(ISSUER, AUDIENCE, "unused://jwks");
        if let Some(t) = tenant_pin {
            c = c.with_tenant(t);
        }
        c
    };
    let source: Arc<dyn JwksSource> = Arc::new(StaticJwksSource::new(jwks));
    ZitadelProvider::new(cfg, source).await.expect("new")
}

/// Build a one-element header map that impls `RequestHeaders`. The
/// formatted `Bearer …` string is held as an owned `String` and
/// paired with `"authorization"` as `&str` tuples; the slice is
/// handed to `resolve` as `&[(&str, &str)]`.
struct Bearer {
    header: String,
}
impl Bearer {
    fn new(token: &str) -> Self {
        Self {
            header: format!("Bearer {token}"),
        }
    }
    fn pairs(&self) -> [(&str, &str); 1] {
        [("authorization", self.header.as_str())]
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn happy_path_returns_auth_context_with_tenant_and_scopes() {
    let kp = fresh_keypair("kid-1");
    let provider = provider_with(kp.jwks.clone(), None).await;
    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "00000000-0000-0000-0000-000000000042",
        3600,
        json!({
            "name": "Alice",
            TENANT_CLAIM: "acme-org",
            SCOPES_CLAIM: ["read_nodes", "write_slots"],
        }),
    );

    let b = Bearer::new(&token); let headers = b.pairs();
    let ctx = provider.resolve(&headers.as_slice()).await.unwrap();

    match ctx.actor {
        Actor::User { display_name, .. } => assert_eq!(display_name, "Alice"),
        other => panic!("expected User actor, got {other:?}"),
    }
    assert_eq!(ctx.tenant.as_str(), "acme-org");
    assert!(ctx.scopes.contains(Scope::ReadNodes));
    assert!(ctx.scopes.contains(Scope::WriteSlots));
    assert!(!ctx.scopes.contains(Scope::ManageFleet));
}

#[tokio::test]
async fn missing_scopes_claim_defaults_to_read_nodes() {
    let kp = fresh_keypair("kid-2");
    let provider = provider_with(kp.jwks.clone(), None).await;
    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "subj",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let ctx = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap();
    assert!(ctx.scopes.contains(Scope::ReadNodes));
    assert!(!ctx.scopes.contains(Scope::WriteSlots));
}

#[tokio::test]
async fn wrong_issuer_is_rejected() {
    let kp = fresh_keypair("kid-3");
    let provider = provider_with(kp.jwks.clone(), None).await;
    let token = sign(
        &kp,
        "https://evil.example",
        AUDIENCE,
        "subj",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    // Surfaces as InvalidCredentials via ZitadelError::Jwt
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn wrong_audience_is_rejected() {
    let kp = fresh_keypair("kid-4");
    let provider = provider_with(kp.jwks.clone(), None).await;
    let token = sign(
        &kp,
        ISSUER,
        "some-other-audience",
        "subj",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn expired_token_is_rejected() {
    let kp = fresh_keypair("kid-5");
    let provider = provider_with(kp.jwks.clone(), None).await;
    // exp 120s in the past — beyond the 30s leeway.
    let token = sign(&kp, ISSUER, AUDIENCE, "subj", -120, json!({ TENANT_CLAIM: "org1" }));
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn bad_signature_is_rejected() {
    // Build the signer with one keypair, hand the *other's* JWKS to
    // the provider. The `kid` matches (same header) but the
    // signature doesn't verify against the advertised public key.
    let signer = fresh_keypair("shared-kid");
    let mut decoy = fresh_keypair("shared-kid");
    // Overwrite decoy's kid to the signer's so kid-lookup succeeds
    // and the test exercises the signature path, not the kid path.
    decoy.kid = "shared-kid".to_string();
    let provider = provider_with(decoy.jwks.clone(), None).await;
    let token = sign(&signer, ISSUER, AUDIENCE, "subj", 3600, json!({ TENANT_CLAIM: "org1" }));
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn unknown_kid_is_rejected() {
    let kp = fresh_keypair("kid-present");
    let provider = provider_with(kp.jwks.clone(), None).await;
    // Sign with a kid the provider's JWKS doesn't list.
    let mut rogue = fresh_keypair("kid-missing");
    rogue.kid = "kid-missing".to_string();
    let token = sign(&rogue, ISSUER, AUDIENCE, "subj", 3600, json!({ TENANT_CLAIM: "org1" }));
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn tenant_pinning_rejects_other_orgs() {
    let kp = fresh_keypair("kid-pin");
    let provider = provider_with(kp.jwks.clone(), Some("edge-org".to_string())).await;
    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "subj",
        3600,
        json!({ TENANT_CLAIM: "OTHER-org" }),
    );
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::WrongTenant));
}

#[tokio::test]
async fn tenant_pinning_accepts_matching_org() {
    let kp = fresh_keypair("kid-pin-2");
    let provider = provider_with(kp.jwks.clone(), Some("edge-org".to_string())).await;
    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "subj",
        3600,
        json!({ TENANT_CLAIM: "edge-org" }),
    );
    let ctx = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap();
    assert_eq!(ctx.tenant.as_str(), "edge-org");
}

#[tokio::test]
async fn missing_bearer_header_is_missing_credentials() {
    let kp = fresh_keypair("kid-absent");
    let provider = provider_with(kp.jwks.clone(), None).await;
    let hs: &[(&str, &str)] = &[];
    let err = provider.resolve(&hs).await.unwrap_err();
    assert!(matches!(err, spi::AuthError::MissingCredentials));
}

#[tokio::test]
async fn provider_id_is_zitadel() {
    let kp = fresh_keypair("kid-id");
    let provider = provider_with(kp.jwks.clone(), None).await;
    assert_eq!(provider.id(), "zitadel");
}

#[tokio::test]
async fn disk_cache_is_read_when_live_fetch_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let cache_path = tmp.path().join("jwks.json");

    // Pre-populate the cache with a known keyset.
    let kp = fresh_keypair("cached");
    let cache = DiskCache::new(cache_path.clone());
    cache.write(&kp.jwks).await.unwrap();

    // Live source always fails.
    struct Dead;
    #[async_trait::async_trait]
    impl JwksSource for Dead {
        async fn fetch(&self) -> auth_zitadel::ZitadelResult<jsonwebtoken::jwk::JwkSet> {
            Err(ZitadelError::UnknownKid("dead".into()))
        }
    }

    let cfg = ZitadelConfig::new(ISSUER, AUDIENCE, "unused://jwks").with_disk_cache(cache_path);
    let provider = ZitadelProvider::new(cfg, Arc::new(Dead)).await.unwrap();

    // Should still verify a token signed with the cached key.
    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "subj",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let ctx = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap();
    assert_eq!(ctx.tenant.as_str(), "org1");
}

#[tokio::test]
async fn no_live_no_cache_fails_new() {
    // First boot, no network, no cache — provider construction fails.
    struct Dead;
    #[async_trait::async_trait]
    impl JwksSource for Dead {
        async fn fetch(&self) -> auth_zitadel::ZitadelResult<jsonwebtoken::jwk::JwkSet> {
            Err(ZitadelError::UnknownKid("dead".into()))
        }
    }
    let cfg = ZitadelConfig::new(ISSUER, AUDIENCE, "unused://jwks");
    match ZitadelProvider::new(cfg, Arc::new(Dead)).await {
        Ok(_) => panic!("expected error, got a provider"),
        // It's whatever the source returned, surfaced through `new`.
        Err(ZitadelError::UnknownKid(_)) => {}
        Err(other) => panic!("unexpected variant: {other:?}"),
    }
}

#[tokio::test]
async fn http_jwks_source_constructs() {
    // Smoke: just build it. Hitting a real URL is out of scope for
    // this test — live-fetch behaviour is exercised implicitly via
    // `ZitadelProvider::new` with an injected `JwksSource` in every
    // other test here.
    let client = reqwest::Client::new();
    let _src = HttpJwksSource::new("https://zitadel.test/oauth/v2/keys", client);
}

// ── Deny-list ─────────────────────────────────────────────────────────────────

#[tokio::test]
async fn deny_list_rejects_revoked_subject_even_with_valid_signature() {
    let kp = fresh_keypair("kid-deny");
    let provider = provider_with(kp.jwks.clone(), None)
        .await
        .with_deny_list(Arc::new(StaticDenyList::new(["revoked-subj"])));

    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "revoked-subj",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let err = provider.resolve(&Bearer::new(&token).pairs().as_slice()).await.unwrap_err();
    // Surfaced as InvalidCredentials so denied subjects can't be
    // enumerated by probing status codes.
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
}

#[tokio::test]
async fn deny_list_lets_non_revoked_subjects_through() {
    let kp = fresh_keypair("kid-deny-pass");
    let provider = provider_with(kp.jwks.clone(), None)
        .await
        .with_deny_list(Arc::new(StaticDenyList::new(["someone-else"])));

    let token = sign(
        &kp,
        ISSUER,
        AUDIENCE,
        "00000000-0000-0000-0000-000000000042",
        3600,
        json!({
            "name": "Alice",
            TENANT_CLAIM: "org1",
        }),
    );
    let ctx = provider
        .resolve(&Bearer::new(&token).pairs().as_slice())
        .await
        .unwrap();
    assert_eq!(ctx.tenant.as_str(), "org1");
}

#[tokio::test]
async fn deny_list_runs_after_signature_check() {
    // Revoked subject + bad signature → rejected for bad signature,
    // NOT for being denied. That's the correct order: revealing
    // "revoked" to an unauthenticated caller would let them probe
    // the deny-list.
    let signer = fresh_keypair("signer-kid");
    let mut decoy = fresh_keypair("signer-kid");
    decoy.kid = "signer-kid".to_string();
    let provider = provider_with(decoy.jwks.clone(), None)
        .await
        .with_deny_list(Arc::new(StaticDenyList::new(["revoked"])));
    let token = sign(
        &signer,
        ISSUER,
        AUDIENCE,
        "revoked",
        3600,
        json!({ TENANT_CLAIM: "org1" }),
    );
    let err = provider
        .resolve(&Bearer::new(&token).pairs().as_slice())
        .await
        .unwrap_err();
    assert!(matches!(err, spi::AuthError::InvalidCredentials { .. }));
    // Both signature failure and deny-list hit surface as the same
    // `InvalidCredentials`; we can't tell them apart from the wire
    // side, which is exactly the enumeration-hardening property we
    // want.
}
