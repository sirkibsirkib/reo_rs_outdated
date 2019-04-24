use std::marker::PhantomData;

pub struct D0<T>(PhantomData<T>);
pub struct D1<T>(PhantomData<T>);
pub struct D2<T>(PhantomData<T>);
pub struct D3<T>(PhantomData<T>);
pub struct D4<T>(PhantomData<T>);
pub struct D5<T>(PhantomData<T>);
pub struct D6<T>(PhantomData<T>);
pub struct D7<T>(PhantomData<T>);
pub struct D8<T>(PhantomData<T>);
pub struct D9<T>(PhantomData<T>);

// convenience to make the last digit less ugly
pub type N0 = D0<()>;
pub type N1 = D1<()>;
pub type N2 = D2<()>;
pub type N3 = D3<()>;
pub type N4 = D4<()>;
pub type N5 = D5<()>;
pub type N6 = D6<()>;
pub type N7 = D7<()>;
pub type N8 = D8<()>;
pub type N9 = D9<()>;

// macro_rules! decimal {
// 	( $( $x:expr ),* ) => {
//         {
//             let mut temp_vec = Vec::new();
//             $(
//                 temp_vec.push($x);
//             )*
//             temp_vec
//         }
//     };
// }

pub type Example42023 = D4<D2<D0<D2<N3>>>>;
