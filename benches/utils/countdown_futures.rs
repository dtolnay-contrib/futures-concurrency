use futures_core::Future;
use pin_project::pin_project;

use std::cell::{Cell, RefCell};
use std::collections::BinaryHeap;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use super::{shuffle, PrioritizedWaker, State};

pub fn futures_vec(len: usize) -> Vec<CountdownFuture> {
    let wakers = Rc::new(RefCell::new(BinaryHeap::new()));
    let completed = Rc::new(Cell::new(0));
    let mut futures: Vec<_> = (0..len)
        .map(|n| CountdownFuture::new(n, len, wakers.clone(), completed.clone()))
        .collect();
    shuffle(&mut futures);
    futures
}

pub fn futures_array<const N: usize>() -> [CountdownFuture; N] {
    let wakers = Rc::new(RefCell::new(BinaryHeap::new()));
    let completed = Rc::new(Cell::new(0));
    let mut futures =
        std::array::from_fn(|n| CountdownFuture::new(n, N, wakers.clone(), completed.clone()));
    shuffle(&mut futures);
    futures
}

pub fn futures_tuple() -> (
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
    CountdownFuture,
) {
    let [f0, f1, f2, f3, f4, f5, f6, f7, f8, f9] = futures_array::<10>();
    (f0, f1, f2, f3, f4, f5, f6, f7, f8, f9)
}

/// A future which will _eventually_ be ready, but needs to be polled N times before it is.
#[pin_project]
pub struct CountdownFuture {
    state: State,
    wakers: Rc<RefCell<BinaryHeap<PrioritizedWaker>>>,
    index: usize,
    max_count: usize,
    completed_count: Rc<Cell<usize>>,
}

impl CountdownFuture {
    pub fn new(
        index: usize,
        max_count: usize,
        wakers: Rc<RefCell<BinaryHeap<PrioritizedWaker>>>,
        completed_count: Rc<Cell<usize>>,
    ) -> Self {
        Self {
            state: State::Init,
            wakers,
            max_count,
            index,
            completed_count,
        }
    }
}
impl Future for CountdownFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();

        // If we are the last stream to be polled, skip strait to the Polled state.
        if this.wakers.borrow().len() + 1 == *this.max_count {
            *this.state = State::Polled;
        }

        match this.state {
            State::Init => {
                // Push our waker onto the stack so we get woken again someday.
                this.wakers
                    .borrow_mut()
                    .push(PrioritizedWaker(*this.index, cx.waker().clone()));
                *this.state = State::Polled;
                Poll::Pending
            }
            State::Polled => {
                // Wake up the next one
                let _ = this
                    .wakers
                    .borrow_mut()
                    .pop()
                    .map(|PrioritizedWaker(_, waker)| waker.wake());

                if this.completed_count.get() == *this.index {
                    *this.state = State::Done;
                    this.completed_count.set(this.completed_count.get() + 1);
                    Poll::Ready(())
                } else {
                    // We're not done yet, so schedule another wakeup
                    this.wakers
                        .borrow_mut()
                        .push(PrioritizedWaker(*this.index, cx.waker().clone()));
                    Poll::Pending
                }
            }
            State::Done => Poll::Ready(()),
        }
    }
}
