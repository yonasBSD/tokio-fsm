use tokio_fsm::{Transition, fsm};

#[fsm(initial = Idle)]
impl MultipleTimeout {
    type Context = ();
    type Error = ();

    #[on(state = Idle, event = Start)]
    async fn start(&mut self) -> Transition<Idle> {
        Transition::to(Idle)
    }

    #[on_timeout]
    async fn timeout1(&mut self) -> Transition<Idle> {
        Transition::to(Idle)
    }

    #[on_timeout]
    async fn timeout2(&mut self) -> Transition<Idle> {
        Transition::to(Idle)
    }
}

fn main() {}
