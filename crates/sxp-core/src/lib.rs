#![no_std]

//! An allocation-free, byte-oriented S-expression lexer.
//!
//! The lexer emits token events with source byte ranges indexing into
//! the input buffer, avoiding memory allocation and copying. It
//! supports streaming contexts, allowing the input buffer to be refilled
//! with new data as it is consumed.

#[cfg(test)]
extern crate std;

mod reader;

pub use reader::{Error, ReadResult, Reader, Token, TokenKind, UnicodeDigits};
