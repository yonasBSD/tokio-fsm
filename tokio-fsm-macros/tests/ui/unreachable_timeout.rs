use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl UnreachableTimeout {
    type Context = ();
    type Error = ();

    #[on(state = Idle, event = Start)]
    async fn start(&mut self) -> Transition<Running> {
        Transition::to(Running)
    }

    #[on_timeout]
    async fn handle_timeout(&mut self) -> Transition<Failed> {
        Transition::to(Failed)
    }
}

fn main() {}
