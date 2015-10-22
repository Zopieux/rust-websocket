//! Module containing the default implementation for messages.

use std::io;
use std::io::Result as IoResult;
use std::io::Write;
use std::iter::{Take, Repeat, repeat};
use result::{WebSocketResult, WebSocketError};
use dataframe::{DataFrame, Opcode, DataFrameRef};
use byteorder::{WriteBytesExt, ReadBytesExt, BigEndian};
use ws::util::message::bytes_to_string;
use ws;

use std::borrow::Cow;

const FALSE_RESERVED_BITS: &'static [bool; 3] = &[false; 3];

/// Represents a WebSocket message.
#[derive(PartialEq, Clone, Debug)]
pub struct Message<'a> {
	pub opcode: Opcode,
	pub cd_status_code: Option<u16>,
	pub payload: Cow<'a, [u8]>,
}

impl<'a> Message<'a> {
	fn new(code: Opcode, status: Option<u16>, payload: Cow<'a, [u8]>) -> Self {
		Message {
			opcode: code,
			cd_status_code: status,
			payload: payload,
		}
	}

	pub fn text<S>(data: S) -> Self
	where S: Into<Cow<'a, str>> {
		Message::new(Opcode::Text, None, match data.into() {
			Cow::Owned(msg) => Cow::Owned(msg.into_bytes()),
			Cow::Borrowed(msg) => Cow::Borrowed(msg.as_bytes()),
		})
	}

	pub fn binary<B>(data: B) -> Self
	where B: Into<Cow<'a, [u8]>> {
		Message::new(Opcode::Binary, None, data.into())
	}

	pub fn close() -> Self {
		Message::new(Opcode::Close, None, Cow::Borrowed(&[0 as u8; 0]))
	}

	pub fn close_because<S>(code: u16, reason: S) -> Self
	where S: Into<Cow<'a, str>> {
		Message::new(Opcode::Close, Some(code), match reason.into() {
			Cow::Owned(msg) => Cow::Owned(msg.into_bytes()),
			Cow::Borrowed(msg) => Cow::Borrowed(msg.as_bytes()),
		})
	}

	pub fn ping<P>(data: P) -> Self
	where P: Into<Cow<'a, [u8]>> {
		Message::new(Opcode::Ping, None, data.into())
	}

	pub fn pong<P>(data: P) -> Self
	where P: Into<Cow<'a, [u8]>> {
		Message::new(Opcode::Pong, None, data.into())
	}
}

impl<'a> ws::dataframe::DataFrame for Message<'a> {
    fn is_last(&self) -> bool {
        true
    }

    fn opcode(&self) -> Opcode {
        self.opcode
    }

    fn reserved<'b>(&'b self) -> &'b [bool; 3] {
		FALSE_RESERVED_BITS
    }

	fn payload<'b>(&'b self) -> &'b [u8] {
		unimplemented!();
	}

    fn write_payload<W>(&self, socket: &mut W) -> IoResult<()>
    where W: Write {
		if let Some(reason) = self.cd_status_code {
			try!(socket.write_u16::<BigEndian>(reason));
		}
		socket.write_all(&*self.payload)
    }
}

impl<'a, 'b> ws::Message<'b, Message<'b>> for Message<'a> {

	type DataFrameIterator = Take<Repeat<Message<'b>>>;

	fn dataframes(&'b self) -> Self::DataFrameIterator {
		repeat(self.clone()).take(1)
    }

	/// Attempt to form a message from a series of data frames
	fn from_dataframes<D>(frames: Vec<D>) -> WebSocketResult<Self>
    where D: ws::dataframe::DataFrame {
		let opcode = try!(frames.first().ok_or(WebSocketError::ProtocolError(
			"No dataframes provided".to_string()
		)).map(|d| d.opcode()));

		let mut data = Vec::new();

		for dataframe in frames.iter() {
			if dataframe.opcode() != Opcode::Continuation {
				return Err(WebSocketError::ProtocolError(
					"Unexpected non-continuation data frame".to_string()
				));
			}
			if *dataframe.reserved() != [false; 3] {
				return Err(WebSocketError::ProtocolError(
					"Unsupported reserved bits received".to_string()
				));
			}
			data.extend(dataframe.payload().iter().cloned());
		}

		Ok(match opcode {
			Opcode::Text => Message::text(try!(bytes_to_string(&data[..]))),
			Opcode::Binary => Message::binary(data),
			Opcode::Close => {
				if data.len() > 0 {
					let status_code = try!((&data[..]).read_u16::<BigEndian>());
					let reason = try!(bytes_to_string(&data[2..]));
					Message::close_because(status_code, reason)
				} else {
					Message::close()
				}
			}
			Opcode::Ping => Message::ping(data),
			Opcode::Pong => Message::pong(data),
			_ => return Err(WebSocketError::ProtocolError(
				"Unsupported opcode received".to_string()
			)),
		})
	}
}
