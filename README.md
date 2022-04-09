# async-into-future

https://twitter.com/yoshuawuyts/status/1512543349806354433

Small proof-of-concept attribute macro that lets you return [`IntoFuture`](https://doc.rust-lang.org/std/future/trait.IntoFuture.html) instead of [`Future`](https://doc.rust-lang.org/std/future/trait.Future.html) from async functions

The utility of such a macro might not immediately be apparent so let's come up with a quick scenario. A non-stealing async executor wants to provide a `spawn` method which will send a future to a background worker thread where it will be polled to completion. The important bit is that once it has been sent to that thread, the future will run until completion without being sent somewhere else. Such a `spawn` function could safely bind its parameter to `Future + Send`, however this would be overly restrictive as the argument only moves threads once, before ever being polled by the executor. But it's also not possible to remove that `Send` bound safely as there is no way to tell the future has never been polled before.

At first glance one solution would be to instead bind the parameter to `(FnOnce() -> Future) + Send` and call the closure from the worker thread. This isn't bad, but it's also not super elegant nor consistent with most spawn functions. The other, (in my opinion) better option is to bind the argument to `IntoFuture + Send` and call `into_future` from the worker thread, which allows just as much flexibility as the closure. In fact, moving the bounds on existing spawn functions from `Future` to `IntoFuture` is not even blocking, as it has a blanket `impl IntoFuture for Future`.

This PoC simply makes it easy to return `IntoFuture + Send` from async functions that would otherwise return a `Future + !Send`. It is very limited (hence the PoC part) and will not work for methods taking a self parameter, or with implicit lifetimes, or in many scenarios that I haven't thought of. Most of these issues aren't fundamental and could be fixed with a bit of code.

```rust
#![feature(into_future)]
use std::{
    future::{Future, IntoFuture},
    ptr::null,
    sync::Arc,
    task::{Context, Poll, Wake, Waker},
    thread::{self, JoinHandle, Thread},
};
use async_into_future::async_into_future;

fn spawn<T: Send + 'static>(f: impl IntoFuture<Output = T> + Send + 'static) -> JoinHandle<T> {
    struct ThreadWaker(Thread);
    impl Wake for ThreadWaker {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }

    thread::spawn(move || {
        let t = thread::current();
        let w = Arc::new(ThreadWaker(t)).into();
        let mut cx = Context::from_waker(&w);
        let mut f = Box::pin(f.into_future());

        loop {
            match f.as_mut().poll(&mut cx) {
                Poll::Ready(res) => break res,
                Poll::Pending => thread::park(),
            }
        }
    })
}

#[async_into_future]
async fn non_send_print<'a>(msg: &'a str) {
    struct NonSend(*const ());
    async fn noop() {}

    // oops! non-send value stays live across an await boundary
    let non_send = NonSend(null());
    noop().await;
    drop(non_send);

    println!("{}", msg);
}

fn main() {
    // this compiles
    let future = async { println!(":)") };
    spawn(future).join().unwrap();

    // this also compiles
    let into_future = non_send_print(":)");
    spawn(into_future).join().unwrap();

    // this fails to compile
    let future = non_send_print(":(").into_future();
    spawn(future).join().unwrap();
}
```
