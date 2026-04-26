//! Example of the Typestate pattern where the compiler is "happy"
//! even though there is a disconnected logical island (StateC).
//!
//! Run with: cargo check --example typestate_logic

struct StateA;
struct StateB;
struct StateC;

// Main logic flow: A <-> B
impl StateA {
    fn next(self) -> StateB {
        StateB
    }
}

impl StateB {
    fn next(self) -> StateA {
        StateA
    }
}

// The "Island": This code is valid and technically "used" by itself,
// but it is logically unreachable from the entry point (StateA).
// Rust compiler doesn't understand the "protocol", so it doesn't complain.
impl StateC {
    fn next(self) -> StateC {
        StateC
    }
}

fn main() {
    println!("Typestate example: checking...");
    let s = StateA;
    let _s = s.next();
    println!("Compiler is happy, but StateC is unreachable and we didn't know!");
}
