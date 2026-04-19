use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl TimeoutNoHandler {
    type Context = ();
    type Error = ();

    #[on(state = Idle, event = Start)]
    #[state_timeout(duration = "1s")]
    async fn start(&mut self) -> Transition<Idle> {
        Transition::to(Idle)
    }
}

fn main() {}
