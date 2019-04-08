
#[derive(Debug)]
pub(crate) struct TokCore;

pub struct TokFinish(TokCore);


#[derive(Debug)]
pub struct T0(TokCore);
#[derive(Debug)]
pub struct T1(TokCore);

trait TokPrivate: Sized {
    fn new() -> Self;
    fn transmute<T: TokPrivate>(self) -> T {
        std::mem::forget(self);
        T::new()
    }
    fn finish(self) -> TokFinish {
        std::mem::forget(self);
        TokFinish(TokCore)
    }
}
impl TokPrivate for T0 {
    fn new() -> Self { T0(TokCore) }
}
impl TokPrivate for T1 {
    fn new() -> Self { T1(TokCore) }
}

pub fn protowang() -> (T0, PutterP0<u32>, GetterP1<u32>) {
    unimplemented!()
}

use crate::threadless2::*;
pub struct PutterP0<T>(crate::threadless2::Putter<T>) where T: TryClone;
impl<T> PutterP0<T> where T: TryClone {
    pub fn try_put(&mut self, tok: T0, datum: T) -> Result<T1, (T1, T)> {
        match self.0.put(datum) {
            Ok(()) => Ok(tok.transmute()),
            Err(datum) => Err((tok.transmute(), datum))
        }
    }
}

pub struct GetterP1<T>(crate::threadless2::Getter<T>) where T: TryClone;
impl<T> GetterP1<T> where T: TryClone {
    pub fn get(&mut self, tok: T1) -> (T0, T) {
        match self.0.get() {
            // tok
            Ok(datum) => (tok.transmute(), datum),
            Err(()) => panic!(),
        }
    }
}