// This file is derived from the jrpc rust crate; original license below.
// It was forked for the following reasons:
//
// 1. That project seemed essentially dead (last update 3 years ago) and had
//    very few lifetime downloads (~3500 per crates.io). This made it likely
//    that we would end up doing maintenance anyhow.
//
// 2. The original project was more general than we needed, e.g., the `Request`
//    struct was generic over the type of the `method`, which made it possible
//    to deserialize directly to an enum of supported methods. Since we don't
//    need this generality, we were able to significantly simplify the code.
//
// 3. The original idea was to use `serde_json` to parse strings into `Request`
//    and `Result` structs. This ends up giving less informative error messages
//    than our approach, and ties us to a strict notion of validity for JSON-RPC
//    messages that we worried would make it harder for us to work around quirks
//    in client and server implementations.
//
// As a high-level summary of the deltas from the original code:
//
// - added `TryFrom` and `From` implementations for most structs;
//
// - removed generics from `Request` type;
//
// - removed `Response` type and renamed `Success` to `Response`;
//
// - made it possible to construct an error with `IdReq::Notification`,
//   which is used for error messages that are not delivered to the client
//   but can be (say) logged by our pipeline.
//
// - updated to more modern usage, fixed clippy warnings, etc.
//
/* The `jrpc` crate's license is as follows:

    The MIT License (MIT)

    Copyright (c) 2018 Garrett Berg, vitiral@gmail.com

    Permission is hereby granted, free of charge, to any person obtaining a copy
    of this software and associated documentation files (the "Software"), to deal
    in the Software without restriction, including without limitation the rights
    to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
    copies of the Software, and to permit persons to whom the Software is
    furnished to do so, subject to the following conditions:

    The above copyright notice and this permission notice shall be included in
    all copies or substantial portions of the Software.

    THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
    IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
    FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
    AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
    LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
    OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
    THE SOFTWARE.
*/

/*
 * # Error codes we use
 *
 * ## Errors that come from network communication
 ****
 * -32700           - parse error (spec defined)
 * -32000 + <STAT>  - got HTTP response code <STAT> from HTTP server
 * -32000           - error from Hyper making HTTP request in Onchain
 * -32001           - error from Hyper getting HTTP response in Onchain
 * -32002           - error converting HTTP response into UTF8
 * -32003           - error converting HTTP response into JSON
 * -32004           - no response
 *
 * ## Errors that come from parsing a Value into a JSON-RPC request/response struct
 ****
 * -29999           - not a JSON-RPC request (but valid JSON)
 * -29998           - JSON-RPC 2.0 required
 * -29997           - vector requests not supported
 * -29996           - malformed numeric request-id
 * -29995           - invalid id type
 * -29994           - method missing or invalid type
 * -29993           - missing response or error
 * -29992           - extraneous fields in otherwise valid JSON-RPC request
 * -29991           - no request-id in response (must exist or be null)
 * -29990           - invalid error response (malformed or missing error object)
 * -29989           - malformed numeric error code
 * -29988           - missing or invalid error code
 * -29987           - missing or invalid error message
 */

//! # jrpc: ultra lightweight types capturing the jsonrpc spec
//!
//! # Specification
//!
//! The below is directly copy/pasted from: [http://www.jsonrpc.org/specification][spec]
//!
//! The types try to correctly copy the relevant documentation snippets in their docstring.
//!
//! [spec]: http://www.jsonrpc.org/specification
//!
//! ## 1 Overview
//!
//! JSON-RPC is a stateless, light-weight remote procedure call (RPC) protocol. Primarily this
//! specification defines several data structures and the rules around their processing. It is
//! transport agnostic in that the concepts can be used within the same process, over sockets, over
//! http, or in many various message passing environments. It uses JSON (RFC 4627) as data format.
//!
//! It is designed to be simple!
//!
//! # 2 Conventions
//!
//! The key words "MUST", "MUST NOT", "REQUIRED", "SHALL", "SHALL NOT", "SHOULD", "SHOULD NOT",
//! "RECOMMENDED", "MAY", and "OPTIONAL" in this document are to be interpreted
//! as described in RFC 2119.
//!
//! Since JSON-RPC utilizes JSON, it has the same type system (see <http://www.json.org> or RFC
//! 4627). JSON can represent four primitive types (Strings, Numbers, Booleans, and Null) and two
//! structured types (Objects and Arrays). The term "Primitive" in this specification references
//! any of those four primitive JSON types. The term "Structured" references either of the
//! structured JSON types. Whenever this document refers to any JSON type, the first letter is
//! always capitalized: Object, Array, String, Number, Boolean, Null. True and False are also
//! capitalized.
//!
//! All member names exchanged between the Client and the Server that are considered for matching
//! of any kind should be considered to be case-sensitive. The terms function, method, and
//! procedure can be assumed to be interchangeable.
//!
//! The Client is defined as the origin of Request objects and the handler of Response objects.
//!
//! The Server is defined as the origin of Response objects and the handler of Request objects.
//!
//! One implementation of this specification could easily fill both of those roles, even at the
//! same time, to other different clients or the same client. This specification does not address
//! that layer of complexity.
//!
//! ## 3 Compatibility
//!
//! JSON-RPC 2.0 Request objects and Response objects may not work with existing JSON-RPC 1.0
//! clients or servers. However, it is easy to distinguish between the two versions as 2.0 always
//! has a member named "jsonrpc" with a String value of "2.0" whereas 1.0 does not. Most 2.0
//! implementations should consider trying to handle 1.0 objects, even if not the peer-to-peer and
//! class hinting aspects of 1.0.
//!
//! ## 4 Request Object
//!
//! See [`Request`](struct.Request.html)
//!
//!
//! ## 4.1 Notification
//!
//! See [`IdReq`](enum.IdReq.html)
//!
//! ## 4.2 Parameter Structures
//!
//! See [`Request.params`](struct.Request.html#structfield.params)
//!
//! ## 5 Response object
//!
//! See [`Response`](struct.Response.html)
//!
//! ## 5.1 Error object
//!
//! See [`ErrorObject`](struct.ErrorObject.html)
//!
//! ## 6 Batch
//!
//! > Note: simply use a `Vec<Request>` and `Vec<Response>`
//!
//! To send several Request objects at the same time, the Client MAY send an Array filled with
//! Request objects.
//!
//! The Server should respond with an Array containing the corresponding Response objects, after
//! all of the batch Request objects have been processed. A Response object SHOULD exist for each
//! Request object, except that there SHOULD NOT be any Response objects for notifications. The
//! Server MAY process a batch rpc call as a set of concurrent tasks, processing them in any order
//! and with any width of parallelism.
//!
//! The Response objects being returned from a batch call MAY be returned in any order within the
//! Array. The Client SHOULD match contexts between the set of Request objects and the resulting
//! set of Response objects based on the id member within each Object.
//!
//! If the batch rpc call itself fails to be recognized as an valid JSON or as an Array with at
//! least one value, the response from the Server MUST be a single Response object. If there are no
//! Response objects contained within the Response array as it is to be sent to the client, the
//! server MUST NOT return an empty Array and should return nothing at all.
//!
//! ## 7 Examples
//!
//! Ommitted. See the [specification][spec]
//!
//! ## 8 Extensions
//!
//! This library does not support checking for extensions. See
//! [`Request.method`](struct.Request.html#structfield.method) for more details of the spec.

use std::{fmt, str};

use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

use crate::JsonRpcErr;

/// Generate a JSON-RPC error message
fn err_val(
    code: impl Into<ErrorCode>,
    message: impl ToString,
    id: impl Into<IdReq>,
    data: Option<impl Into<Value>>,
) -> Error {
    Error::new(id.into(), code.into(), message, data.map(Into::into))
}

/// Generate a "no-response" error with the provided string data
pub fn no_response(data: Option<impl Into<Value>>) -> JsonRpcErr {
    err_val(-32004, "no response", IdReq::Notification, data).into()
}

/// Generate a response for request-id `id` with the given `result`.
/// If `id` is `IdReq::Notification`, a no-response Error is generated instead,
/// allowing the response to be logged and then discarded before sending.
pub fn response(id: impl Into<IdReq>, result: Value) -> Result<Value, JsonRpcErr> {
    let id = id.into();
    if id.is_notification() {
        Err(no_response(Some(result)))
    } else {
        let id = id.try_into().expect("should not be notification");
        Ok(Response::new(id, result).into())
    }
}

/// Generate a JSON-RPC error message with the provided string data
pub fn error(
    code: impl Into<ErrorCode>,
    message: impl ToString,
    id: impl Into<IdReq>,
    data: impl ToString,
) -> JsonRpcErr {
    err_val(code, message, id, Some(Value::String(data.to_string()))).into()
}

/// Generate a JSON-RPC parse error message
pub fn parse_error(data: impl ToString) -> JsonRpcErr {
    error(ErrorCode::ParseError, "Parse error", IdReq::Null, data)
}

/// The `jsonrpc` version. Will serialize/deserialize to/from `"2.0"`.
#[derive(PartialEq, Eq, Copy, Clone, Serialize, Deserialize)]
#[serde(try_from = "String")]
#[serde(into = "String")]
pub struct V2_0;

impl fmt::Debug for V2_0 {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("\"2.0\"")
    }
}

impl TryFrom<String> for V2_0 {
    type Error = String;

    fn try_from(other: String) -> Result<Self, Self::Error> {
        match &other[..] {
            "2.0" => Ok(Self),
            other => Err(format!("V2_0: expected \"2.0\", found \"{other}\"")),
        }
    }
}

impl From<V2_0> for String {
    fn from(_: V2_0) -> Self {
        "2.0".to_owned()
    }
}

/// An identifier established by the Client that MUST contain a String, Number, or NULL value if
/// included. If it is not included it is assumed to be a notification. The value SHOULD normally
/// not be Null and Numbers SHOULD NOT contain fractional parts
///
/// The Server MUST reply with the same value in the Response object if included. This member is
/// used to correlate the context between the two objects.
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Id {
    /// An String id
    String(String),
    /// An Number id that must be an integer.
    ///
    /// We intentionally do not allow floating point values.
    Int(i64),
    /// A null id
    Null,
}

impl From<String> for Id {
    fn from(s: String) -> Self {
        Id::String(s)
    }
}

impl<'a> From<&'a str> for Id {
    fn from(s: &'a str) -> Self {
        Id::String(s.into())
    }
}

impl From<i64> for Id {
    fn from(v: i64) -> Self {
        Id::Int(v)
    }
}

impl TryFrom<IdReq> for Id {
    type Error = ();

    fn try_from(other: IdReq) -> Result<Self, ()> {
        match other {
            IdReq::String(s) => Ok(Id::String(s)),
            IdReq::Int(i) => Ok(Id::Int(i)),
            IdReq::Null => Ok(Id::Null),
            IdReq::Notification => Err(()),
        }
    }
}

impl TryFrom<Value> for Id {
    type Error = Error;

    fn try_from(other: Value) -> Result<Self, Error> {
        match other {
            Value::String(id) => Ok(Id::String(id)),
            Value::Number(n) => match n.as_i64() {
                Some(n) => Ok(Id::Int(n)),
                None => Err(Error::new(
                    Id::Null,
                    -29996,
                    "malformed numeric id",
                    Some(Value::Number(n)),
                )),
            },
            Value::Null => Ok(Id::Null),
            id => Err(Error::new(Id::Null, -29995, "invalid id", Some(id))),
        }
    }
}

// This impl is used in `<Error as TryFrom<Value>>::try_from` and
// `<Response as TryFrom<Value>>::try_from`.
impl TryFrom<Option<Value>> for Id {
    type Error = Error;

    fn try_from(other: Option<Value>) -> Result<Self, Error> {
        match other {
            None => Err(Error::new(Id::Null, -29991, "no id in response", None)),
            Some(v) => Ok(Id::try_from(v)?),
        }
    }
}

/// Identical to [`Id`](enum.Id.html) except has the Notification type. Typically you should use
/// `Id` since all functions that would accept IdReq accept `Into<IdReq>`.
///
/// # Notification
///
/// A Notification is a Request object without an "id" member. A Request object that is a
/// Notification signifies the Client's lack of interest in the corresponding Response object, and
/// as such no Response object needs to be returned to the client. The Server MUST NOT reply to a
/// Notification, including those that are within a batch request.
///
/// Notifications are not confirmable by definition, since they do not have a Response object to be
/// returned. As such, the Client would not be aware of any errors (like e.g. "Invalid
/// params","Internal error").
///
/// <https://github.com/serde-rs/serde/issues/984>
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IdReq {
    /// An String id
    String(String),
    /// An Number id that must be an integer.
    ///
    /// We intentionally do not allow floating point values.
    Int(i64),
    /// A null id
    Null,
    /// The notification id, i.e. the id is absent.
    Notification,
}

impl IdReq {
    // Return whether the `id` is a `Notification`.
    //
    // Per JSON-RPC-2.0-Section-4.1, we must exclude the `id` field in this case.
    //
    // This function is used by serde; see annotation on the Request struct def'n
    pub fn is_notification(&self) -> bool {
        matches!(self, IdReq::Notification)
    }
}

impl<T: Into<Id>> From<T> for IdReq {
    fn from(id: T) -> Self {
        match id.into() {
            Id::String(s) => IdReq::String(s),
            Id::Int(i) => IdReq::Int(i),
            Id::Null => IdReq::Null,
        }
    }
}

// This impl is used in `<Request as TryFrom<Value>>::try_from`.
impl TryFrom<Option<Value>> for IdReq {
    type Error = Error;

    fn try_from(other: Option<Value>) -> Result<Self, Error> {
        match other {
            None => Ok(IdReq::Notification),
            Some(v) => Ok(Id::try_from(v)?.into()),
        }
    }
}

/// A rpc call is represented by sending a Request object to a Server.
///
/// See the parameters for details.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Request {
    /// A String specifying the version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: V2_0,

    /// A serializable method.
    ///
    /// The spec states it must be a String containing the name of the method to be invoked. This
    /// library makes no guarantees about this. It is recomended to use a simple `enum` for your
    /// library's `method`.
    ///
    /// ## Section 8: Extensions
    ///
    /// Method names that begin with `"rpc."` are reserved for system extensions, and MUST NOT be
    /// used for anything else. Each system extension is defined in a related specification. All
    /// system extensions are OPTIONAL.
    ///
    /// This library provides no way of checking for system extensions.
    pub method: String,

    /// A Structured value that holds the parameter values to be used during the invocation of the
    /// method.
    ///
    /// ## Spec Requirement
    ///
    /// > Note: the following spec is **not** upheld by this library.
    ///
    /// If present, parameters for the rpc call MUST be provided as a Structured value. Either
    /// by-position through an Array or by-name through an Object.
    ///
    /// - by-position: params MUST be an Array, containing the values in the Server expected
    ///   order.
    /// - by-name: params MUST be an Object, with member names that match the Server expected
    ///   parameter names. The absence of expected names MAY result in an error being
    ///   generated. The names MUST match exactly, including case, to the method's expected
    ///   parameters.
    ///
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Value>,

    /// The `id`. See [`Id`](enum.Id.html)
    #[serde(default = "notification")]
    #[serde(skip_serializing_if = "IdReq::is_notification")]
    pub id: IdReq,
}

// used by serde
fn notification() -> IdReq {
    IdReq::Notification
}

impl fmt::Display for Request {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

impl str::FromStr for Request {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

// extract a map from a Value for conversion to Request or Response, or return an error
fn jrpc_value_map(value: Value) -> Result<Map<String, Value>, Error> {
    let mut map = match value {
        Value::Object(map) => Ok(map),
        aa @ Value::Array(_) => Err(Error::new(
            Id::Null,
            -29997,
            "vector requests unsupported",
            Some(aa),
        )),
        v => Err(Error::new(
            Id::Null,
            -29999,
            "not a JSON-RPC request",
            Some(v),
        )),
    }?;

    match map.remove("jsonrpc") {
        Some(Value::String(tpo)) if &tpo == "2.0" => Ok(map),
        jsonrpc => Err(Error::new(
            Id::Null,
            -29998,
            "JSON-RPC 2.0 required",
            jsonrpc,
        )),
    }
}

impl TryFrom<Value> for Request {
    type Error = JsonRpcErr;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        // this fn call checks the jsonrpc field, too
        let mut map = jrpc_value_map(value)?;
        let id: IdReq = map.remove("id").try_into()?;

        // check method field
        let method = match map.remove("method") {
            Some(Value::String(method)) => method,
            mm => return Err(err_val(-29994, "method missing or invalid", id, mm).into()),
        };

        // check parameters
        let params = match map.remove("params") {
            None => None,
            Some(v) if matches!(v, Value::Array(_) | Value::Object(_)) => Some(v),
            pp => return Err(err_val(ErrorCode::InvalidParams, "invalid params", id, pp).into()),
        };

        // shouldn't have any remaining params
        require_empty_map(map, &id)?;
        Ok(Self {
            jsonrpc: V2_0,
            method,
            params,
            id,
        })
    }
}

macro_rules! value_from {
    ($fty: ty) => {
        impl From<$fty> for Value {
            fn from(other: $fty) -> Value {
                json!(other)
            }
        }
        impl From<&$fty> for Value {
            fn from(other: &$fty) -> Value {
                json!(other)
            }
        }
    };
}

value_from!(Request);

impl Request {
    /// Create a new Request.
    pub fn new(id: impl Into<IdReq>, method: impl ToString) -> Self {
        Self {
            jsonrpc: V2_0,
            method: method.to_string(),
            params: None,
            id: id.into(),
        }
    }

    /// Create a new Request with the specified params.
    pub fn with_params(
        id: impl Into<IdReq>,
        method: impl ToString,
        params: impl Into<Value>,
    ) -> Self {
        Self {
            jsonrpc: V2_0,
            method: method.to_string(),
            params: Some(params.into()),
            id: id.into(),
        }
    }
}

/// The jsonrpc Response response, indicating a successful result.
///
/// See the parameters for more information.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Response {
    /// A String specifying the version of the JSON-RPC protocol. MUST be exactly "2.0".
    pub jsonrpc: V2_0,

    /// The value of this member is determined by the method invoked on the Server.
    pub result: Value,

    /// This member is REQUIRED.
    ///
    /// It MUST be the same as the value of the id member in the Request Object.
    ///
    /// If there was an error in detecting the id in the Request object (e.g. Parse error/Invalid
    /// Request), it MUST be Null.
    pub id: Id,
}

impl Response {
    /// Construct a `Response`, i.e. a Response with a `result` object.
    pub fn new(id: Id, result: Value) -> Self {
        Self {
            jsonrpc: V2_0,
            result,
            id,
        }
    }
}

impl str::FromStr for Response {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

impl fmt::Display for Response {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

impl TryFrom<Value> for Response {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        // this fn call checks the jsonrpc field, too
        let mut map = jrpc_value_map(value)?;
        // responses *must* have an id field
        let id: Id = map.remove("id").try_into()?;

        let res = if let Some(res) = map.remove("result") {
            Response::new(id, res)
        } else {
            return Err(err_val(
                -29993,
                "missing response",
                id,
                Some(Value::Object(map)),
            ));
        };

        require_empty_map(map, &res.id)?;
        Ok(res)
    }
}

value_from!(Response);

/// Convert a Value into either a Response or an Error, or throw JsonRpcErr if something goes
/// wrong in the conversion.
#[allow(dead_code)]
pub fn into_response_or_error(value: Value) -> Result<Result<Response, Error>, JsonRpcErr> {
    // this fn call checks the jsonrpc field, too
    let mut map = jrpc_value_map(value)?;
    // responses *must* have an id field
    let id: Id = map.remove("id").try_into()?;

    let res = if map.contains_key("error") {
        Err(try_error(&mut map, id.clone())?)
    } else if let Some(res) = map.remove("result") {
        Ok(Response::new(id.clone(), res))
    } else {
        return Err(err_val(
            -29993,
            "missing response or error",
            id,
            Some(Value::Object(map)),
        )
        .into());
    };

    require_empty_map(map, &id)?;
    Ok(res)
}

/// The jsonrpc Error response, indicating an error.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Error {
    /// Always "2.0"
    pub jsonrpc: V2_0,
    /// The error object.
    pub error: ErrorObject,
    /// The id of the request. We allow an IdReq here even though a Notification
    /// cannot produce an Error response to let us track errors that result from
    /// Notification requests and other instances where we send no response.
    pub id: IdReq,
}

impl Error {
    /// Helper to create a new `Error` object.
    pub fn new(
        id: impl Into<IdReq>,
        code: impl Into<ErrorCode>,
        message: impl ToString,
        data: Option<Value>,
    ) -> Self {
        Error {
            jsonrpc: V2_0,
            error: ErrorObject {
                code: code.into(),
                message: message.to_string(),
                data,
            },
            id: id.into(),
        }
    }

    /// Set this error's id field to `id`
    pub fn with_id<I: Into<IdReq> + Clone>(mut self, id: &I) -> Self {
        self.id = id.clone().into();
        self
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", serde_json::to_string(self).unwrap())
    }
}

impl std::error::Error for Error {}

impl str::FromStr for Error {
    type Err = serde_json::Error;

    fn from_str(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

// NOTE does not ensure that `map` is empty at the end
//      call require_empty_map afterwards!
//
/// Attempt to construct an `Error` value from a `serde_json::Map` whose
/// `id` value has already been extracted and passed along as an argument.
///
/// This function breaks out code common to `<Error as TryFrom<Value>>::try_from`
/// and `into_respones_or_error`.
fn try_error(map: &mut Map<String, Value>, id: Id) -> Result<Error, Error> {
    let mut err_map = match map.remove("error") {
        Some(Value::Object(o)) => o,
        err => return Err(Error::new(id, -29990, "invalid error response", err)),
    };

    let code = ErrorCode::try_from(err_map.remove("code")).map_err(|e| e.with_id(&id))?;
    let data = err_map.remove("data");

    match err_map.remove("message") {
        Some(Value::String(msg)) => Ok(Error::new(id, code, msg, data)),
        bad => Err(Error::new(
            id,
            -29987,
            "missing or invalid error message",
            bad,
        )),
    }
}

fn require_empty_map(
    map: Map<String, Value>,
    id: &(impl Into<IdReq> + Clone),
) -> Result<(), Error> {
    if !map.is_empty() {
        Err(err_val(
            -29992,
            "extraneous fields",
            id.clone(),
            Some(Value::Object(map)),
        ))
    } else {
        Ok(())
    }
}

// This implementation requires a well-formed Error, i.e., one that
// has an `id` field, even though our `Error` type can represent an
// error in response to a JSON-RPC notification. This is because a
// "real" Error object should not have this form.
impl TryFrom<Value> for Error {
    type Error = Error;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        // this fn call checks the jsonrpc field, too
        let mut map = jrpc_value_map(value)?;
        // errors *must* have an id field
        let id: Id = map.remove("id").try_into()?;

        let ret = try_error(&mut map, id)?;
        require_empty_map(map, &ret.id)?;
        Ok(ret)
    }
}

value_from!(Error);

/// The jsonrpc Error object, with details of the error.
///
/// When a rpc call encounters an error, the Response Object MUST contain the error member with a
/// value that is a Object. See the attributes for details.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ErrorObject {
    /// The error code. See [`ErrorCode`](enum.ErrorCode.html)
    pub code: ErrorCode,

    /// A String providing a short description of the error.
    ///
    /// The message SHOULD be limited to a concise single sentence.
    pub message: String,

    /// A Primitive or Structured value that contains additional information about the error.
    ///
    /// This may be omitted.
    ///
    /// The value of this member is defined by the Server (e.g. detailed error
    /// information, nested errors etc.).
    #[serde(default)]
    pub data: Option<Value>,
}

/// A Number that indicates the error type that occurred.
/// This MUST be an integer.
///
/// The error codes from and including -32768 to -32000 are reserved for pre-defined errors.
/// Any code within this range, but not defined explicitly below is reserved for future use.
/// The error codes are nearly the same as those suggested for XML-RPC at the following url:
/// <http://xmlrpc-epi.sourceforge.net/specs/rfc.fault_codes.php>
///
/// Use the [`is_valid()`](enum.ErrorCode.html#method.is_valid) method to determine compliance.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(from = "i64")]
#[serde(into = "i64")]
pub enum ErrorCode {
    /// - `-32700`: Parse error. Invalid JSON was received by the server.
    ///   An error occurred on the server while parsing the JSON text.
    ParseError,
    /// - `-32600`: Invalid Request. The JSON sent is not a valid Request object.
    InvalidRequest,
    /// - `-32601`: Method not found. The method does not exist / is not available.
    MethodNotFound,
    /// - `-32602`: Invalid params. Invalid method parameter(s).
    InvalidParams,
    /// - `-32603`: Internal error. Internal JSON-RPC error.
    InternalError,
    /// - `-32001`: An error occurred while trying to generate a nonce for a transaction.
    NonceGenerationError,
    /// - `-32002`: An error occurred when signing a transaction.
    SigningError,
    /// - `-32003`: An error occurred when estimating gas prices
    GasEstimationError,
    /// - `-32000 to -32099`: Server error. Reserved for implementation-defined server-errors.
    ServerError(i64),
    /// - Any error outside the -32700..=-32000 is allowed.
    Other(i64),
}

impl ErrorCode {
    /// Return whether the ErrorCode is correct.
    ///
    /// This will only return `false` if this is `ServerError` and is outside of the range of -32000
    /// to -32099.
    pub fn is_valid(&self) -> bool {
        match *self {
            ErrorCode::ServerError(value) => (-32099..=-32000).contains(&value),
            _ => true,
        }
    }
}

impl From<i64> for ErrorCode {
    fn from(v: i64) -> ErrorCode {
        match v {
            -32700 => ErrorCode::ParseError,
            -32600 => ErrorCode::InvalidRequest,
            -32601 => ErrorCode::MethodNotFound,
            -32602 => ErrorCode::InvalidParams,
            -32603 => ErrorCode::InternalError,
            -32001 => ErrorCode::NonceGenerationError,
            -32002 => ErrorCode::SigningError,
            -32003 => ErrorCode::GasEstimationError,
            v if (-32099..=-32000).contains(&v) => ErrorCode::ServerError(v),
            _ => ErrorCode::Other(v),
        }
    }
}

impl From<ErrorCode> for i64 {
    fn from(c: ErrorCode) -> Self {
        match c {
            ErrorCode::ParseError => -32700,
            ErrorCode::InvalidRequest => -32600,
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::InternalError => -32603,
            ErrorCode::NonceGenerationError => -32001,
            ErrorCode::SigningError => -32002,
            ErrorCode::GasEstimationError => -32003,
            ErrorCode::ServerError(value) => value,
            ErrorCode::Other(value) => value,
        }
    }
}

// This impl is used in `try_error`.
impl TryFrom<Option<Value>> for ErrorCode {
    type Error = Error;

    fn try_from(other: Option<Value>) -> Result<Self, Error> {
        match other {
            Some(Value::Number(n)) => match n.as_i64() {
                Some(n) => Ok(ErrorCode::from(n)),
                None => Err(Error::new(
                    Id::Null,
                    -29989,
                    "malformed numeric error code",
                    Some(Value::Number(n)),
                )),
            },
            bad => Err(Error::new(
                Id::Null,
                -29988,
                "missing or invalid error code",
                bad,
            )),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    use serde_json::{json, Value};

    /// Parse a json string, returning either:
    /// - The parsed `Request`
    /// - An `Error` object created according to the jsonrpc spec (with a _useful_ reason/message).
    ///
    /// This parses the json in stages and will correctly return one of the following errors on
    /// failure:
    ///
    /// - `ParseError`
    /// - `InvalidRequest`
    /// - `MethodNotFound`
    ///
    /// > Reminder: It is up to the user to return the `InvalidParams` error if the `request.params` is
    /// > invalid.
    fn parse_request(json: &str) -> Result<Request, Error> {
        let value: Value = serde_json::from_str(json).map_err(|err| {
            Error::new(
                Id::Null,
                ErrorCode::ParseError,
                err.to_string(),
                Some(Value::String(json.to_owned())),
            )
        })?;

        Request::try_from(value).map_err(|e| match e {
            JsonRpcErr::Jrpc(e) => e,
            _ => unreachable!(),
        })
    }

    #[test]
    fn test_id() {
        let id: Id = serde_json::from_str("1").unwrap();
        assert_eq!(id, Id::Int(1));

        let id: Id = serde_json::from_str("\"1\"").unwrap();
        assert_eq!(id, Id::String("1".into()));

        let id: Id = serde_json::from_str("null").unwrap();
        assert_eq!(id, Id::Null);
    }

    #[test]
    fn test_notification_id() {
        let value = json!([1, 2, 3]);
        let request =
            Request::with_params(IdReq::Notification, "CreateFoo".to_string(), Some(value));
        let json = r#"
    {
      "jsonrpc":"2.0",
      "method":"CreateFoo",
      "params":[1,2,3]
    }
    "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&request).unwrap();
        assert_eq!(json, result);
    }

    #[test]
    fn test_request_id() {
        let value = json!([1, 2, 3]);
        let request = Request::with_params(Id::from(7), "CreateFoo".to_string(), Some(value));
        let json = r#"
    {
      "jsonrpc":"2.0",
      "method":"CreateFoo",
      "params":[1,2,3],
      "id": 7
    }
    "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&request).unwrap();
        assert_eq!(json, result);
    }

    #[test]
    fn test_from_tryfrom_request() {
        let value = json!([1, 2, 3]);

        let request = Request::with_params(Id::from(1), "my_method", Some(value));
        let value: Value = request.clone().into();
        let request2 = Request::try_from(value).unwrap();
        assert_eq!(request, request2);
    }

    #[test]
    fn test_from_tryfrom_response() {
        let data = json!([1, 2, 3]);

        let response = Response::new(Id::Null, data.clone());
        let value: Value = response.clone().into();
        let value2 = json!({
            "id": null,
            "jsonrpc": "2.0",
            "result": [1,2,3],
        });
        assert_eq!(value, value2);
        let response2 = Response::try_from(value).unwrap();
        assert_eq!(response, response2);

        let error = Error::new(Id::Null, 1024, "asdf", Some(data));
        let value: Value = error.clone().into();
        let value2 = json!({
            "id": null,
            "jsonrpc": "2.0",
            "error": {
                "code": 1024,
                "message": "asdf",
                "data": [1,2,3],
            }
        });
        assert_eq!(value, value2);
        let error2 = Error::try_from(value).unwrap();
        assert_eq!(error, error2);
    }

    #[test]
    fn example_id() {
        assert_eq!(Id::from(4), Id::Int(4));
        assert_eq!(serde_json::from_str::<Id>("4").unwrap(), Id::Int(4),);
        assert_eq!(
            serde_json::from_str::<Id>("\"foo\"").unwrap(),
            Id::String("foo".into()),
        );
        assert_eq!(serde_json::from_str::<Id>("null").unwrap(), Id::Null,);
    }

    #[test]
    fn example_idreq() {
        let json = r#"
        {
            "jsonrpc": "2.0",
            "method": "CreateFoo",
            "id": null
        }
        "#;
        let request: Request = serde_json::from_str(json).unwrap();
        assert_eq!(request.id, Id::Null.into());

        // id does not exist
        let json = r#"
        {
            "jsonrpc": "2.0",
            "method": "NotifyFoo"
        }
        "#;
        let request: Request = serde_json::from_str(json).unwrap();
        assert_eq!(request.id, IdReq::Notification);
    }

    #[test]
    fn example_request_new() {
        let request = Request::new(Id::from(4), "CreateFoo".to_string());
        println!("{}", request);
    }

    #[test]
    fn example_request_new_2() {
        let value = json!([1, 2, 3]);
        let request = Request::with_params(Id::from(4), "CreateFoo".to_string(), Some(value));
        let json = r#"
        {
            "jsonrpc": "2.0",
            "method": "CreateFoo",
            "params": [1,2,3],
            "id": 4
        }
        "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&request).unwrap();
        assert_eq!(json, result);
    }

    #[test]
    fn example_success_new() {
        let data = json!([1, 2, 3]);
        let example = Response::new(Id::from(4), data);
        let json = r#"
        {
            "jsonrpc": "2.0",
            "result": [1,2,3],
            "id": 4
        }
        "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&example).unwrap();
        assert_eq!(json, result);
    }

    #[test]
    fn example_request_good() {
        let params = json!([1, 2, 3]);
        let request =
            Request::with_params(Id::from(4), "CreateFoo".to_string(), Some(params.clone()));
        let json = r#"
        {
            "jsonrpc": "2.0",
            "method": "CreateFoo",
            "params": [1,2,3],
            "id": 4
        }
        "#;

        let result: Request = parse_request(json).unwrap();
        let result_params: Vec<u32> = serde_json::from_value(result.params.unwrap()).unwrap();
        assert_eq!(
            serde_json::from_value::<Vec<u32>>(params).unwrap(),
            result_params
        );
        assert_eq!(request.method, result.method);
        assert_eq!(request.id, result.id);
    }

    #[test]
    fn example_request_parse_error() {
        let json = r#"
        Not Valid JSON...
        "#;

        let result: Result<Request, Error> = parse_request(json);
        let error = result.unwrap_err();
        assert_eq!(error.error.code, ErrorCode::ParseError);
    }

    #[test]
    fn example_request_other_error() {
        let json = r#"
        {
            "type": "valid json",
            "but": "not jsonrpc!"
        }
        "#;

        let result: Result<Request, Error> = parse_request(json);
        let error = result.unwrap_err();
        assert_eq!(error.error.code, ErrorCode::Other(-29998));
        assert!(error.error.message.contains("JSON-RPC 2.0 required"));
        assert_eq!(error.id, IdReq::Null);
    }

    #[test]
    fn example_response_success() {
        let data = json!([1, 2, 3]);
        let example = Response::new(Id::from(4), data);
        let json = r#"
        {
            "jsonrpc": "2.0",
            "result": [1,2,3],
            "id": 4
        }
        "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&example).unwrap();
        assert_eq!(json, result);
    }

    #[test]
    fn example_response_error() {
        let data = json!([1, 2, 3]);
        let example = Error {
            jsonrpc: V2_0,
            error: ErrorObject {
                code: ErrorCode::from(-32000),
                message: "BadIndexes".into(),
                data: Some(data.clone()),
            },
            id: Id::from(4).into(),
        };

        let json = r#"
        {
            "jsonrpc": "2.0",
            "error": {
                "code": -32000,
                "message": "BadIndexes",
                "data": [1,2,3]
            },
            "id": 4
        }
        "#;
        let json = json.replace(['\n', ' '], "");
        let result = serde_json::to_string(&example).unwrap();
        assert_eq!(json, result);

        // This is how it is recommended you deserialize:
        let error: Error = serde_json::from_str(&json).unwrap();
        if error.error.code != ErrorCode::ServerError(-32000) {
            panic!("unexpected error");
        }
        let result: Vec<u32> = serde_json::from_value(error.error.data.unwrap()).unwrap();
        assert_eq!(serde_json::from_value::<Vec<u32>>(data).unwrap(), result);
    }
}
