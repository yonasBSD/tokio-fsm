use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl DuplicateSourceStateFsm {
    type Context = ();
    type Error = std::convert::Infallible;

    #[on(state = Idle, event = Start)]
    #[on(state = Idle, event = Start)]
    async fn on_start(&mut self) -> Transition<Running> {
        Transition::to(Running)
    }
}

fn main() {}
