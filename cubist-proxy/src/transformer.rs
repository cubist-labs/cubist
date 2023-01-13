// Copyright 2022 Riad S. Wahby <r@cubist.dev> and the Cubist developers
//
// This file is part of cubist-proxy.
//
// See LICENSE for licensing terms. This file may not be copied,
// modified, or distributed except according to those terms.

//! transformer: a (stream/sink) <-> (stream/sink) adapter

mod convert;
mod copy;
mod error;
pub mod eth_creds;
mod switch;
mod trace;
mod uri;

pub use convert::{convert, convert_fut, json};
pub use copy::copy;
pub use error::{errors_to_sink, errors_to_stream};
pub use switch::{switch, switch_convert_stream, switch_stream};
pub use trace::debug;
pub use uri::{canon_request, canon_request_stream};
