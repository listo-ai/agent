
    export type RemoteKeys = 'REMOTE_ALIAS_IDENTIFIER/registry';
    type PackageType<T> = T extends 'REMOTE_ALIAS_IDENTIFIER/registry' ? typeof import('REMOTE_ALIAS_IDENTIFIER/registry') :any;