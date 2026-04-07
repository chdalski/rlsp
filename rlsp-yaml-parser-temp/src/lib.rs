// SPDX-License-Identifier: MIT

mod chars;
mod error;
mod event;
mod lexer;
mod lines;
mod loader;
mod pos;
mod scanner;

pub fn parse_events(input: &str) -> impl Iterator<Item = ()> + '_ {
    let _ = input;
    std::iter::empty()
}
