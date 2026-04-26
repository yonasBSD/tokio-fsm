//! Example of tokio-fsm detecting a disconnected logical island.
//!
//! Run with: cargo check --example fsm_logic
//! (Expected to fail)

use tokio_fsm::{fsm, Transition};

pub struct Context;

#[fsm(initial = StateA)]
impl MyFsm {
    type Context = Context;
    type Error = std::convert::Infallible;

    // Main logic flow: A -> B -> A
    #[on(state = StateA, event = Next)]
    async fn to_b(&mut self) -> Transition<StateB> {
        Transition::to(StateB)
    }

    #[on(state = StateB, event = Next)]
    async fn to_a(&mut self) -> Transition<StateA> {
        Transition::to(StateA)
    }

    // The "Island": tokio-fsm will detect that StateC is unreachable from StateA!
    #[on(state = StateC, event = Next)]
    async fn to_c(&mut self) -> Transition<StateC> {
        Transition::to(StateC)
    }
}

fn main() {
    println!("FSM example: checking...");
}
