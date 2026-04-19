use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl MixedEvents {
    type Context = ();
    type Error = ();

    #[on(state = Idle, event = Dummy)]
    async fn dummy(&mut self) -> Transition<Running> {
        Transition::to(Running)
    }

    #[on(state = Idle, event = Start)]
    #[on(state = Running, event = Stop)]
    async fn start_or_stop(&mut self) -> Transition<Idle> {
        Transition::to(Idle)
    }
}

fn main() {}
