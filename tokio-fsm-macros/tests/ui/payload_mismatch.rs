use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl PayloadMismatch {
    type Context = ();
    type Error = ();

    #[on(state = Idle, event = Start)]
    async fn start1(&mut self, data: String) -> Transition<Idle> {
        Transition::to(Idle)
    }

    #[on(state = Idle, event = Start)]
    async fn start2(&mut self, data: u32) -> Transition<Idle> {
        Transition::to(Idle)
    }
}

fn main() {}
