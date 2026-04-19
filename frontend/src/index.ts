// Async-bootstrap pattern required by Module Federation.
//
// The entry file must NOT import any shared dep synchronously — doing so
// triggers MF RUNTIME-006 ("Invalid loadShareSync function call from runtime")
// because the share scope isn't initialised when the import resolves.
//
// Everything React-touching lives in ./bootstrap; this file just defers it
// until MF has wired up the shared singletons.
import("./bootstrap");

export {};
