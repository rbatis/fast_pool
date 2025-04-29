pub struct Guard<F: FnMut()>(pub Option<F>);

impl<F: FnMut()> Drop for Guard<F> {
    fn drop(&mut self) {
        if let Some(mut f) = (self.0).take() {
            f()
        }
    }
}

macro_rules! defer {
    ($func:block) => {
       let _guard = $crate::defer::Guard(Some( ||$func));
    };
    ($func:expr) => {
        let _guard = $crate::defer::Guard(Some($func));
    };
    { $($func:expr$(;)?)+ } => {
       let _guard = $crate::defer::Guard(Some( ||{$($func;)+}));
    }
}
