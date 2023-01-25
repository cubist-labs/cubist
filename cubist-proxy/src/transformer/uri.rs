//! URI modifying transformers

use futures::TryStreamExt;
use hyper::{http::request::Parts, Request, Uri};

use crate::Pair;

/// Canonicalize the URIs in a stream of requests by applying `canon_request` to each,
/// where `uri` provides the scheme and authority for the resulting URI.
pub fn canon_request_stream<Si, B, Ei>(
    pair: impl Pair<Si, Request<B>, Ei>,
    uri: Uri,
) -> impl Pair<Si, Request<B>, Ei> {
    pair.map_ok(move |req: Request<B>| {
        let (parts, body) = req.into_parts();
        canon_request(parts, body, uri.clone())
    })
}

/// Create a Request by canonicalizing a URI
///
/// This function generates a `Request` whose URI combines the scheme and authority
/// from `uri` with the path and query from `parts.uri`.
///
/// # Arguments
/// - `parts` - the `Parts` of the resulting Request, except that the scheme and authority
///             from `parts.uri is ignored.
/// - `body` - the body of the resulting Request.
/// - `uri` - the scheme (i.e., "http", "wss", ...) and authority (e.g., "mynode.xyz:1357")
///           of thhe URI in the resulting Request. The path and query from this URI are
///           ignored.
pub fn canon_request<B>(mut parts: Parts, body: B, uri: Uri) -> Request<B> {
    let mut uri_parts = uri.into_parts();
    uri_parts.path_and_query = parts.uri.into_parts().path_and_query;
    parts.uri = Uri::from_parts(uri_parts).expect("valid URI parts");
    Request::from_parts(parts, body)
}
