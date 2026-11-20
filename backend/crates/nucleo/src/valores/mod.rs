//! Objetos de valor: se validan al construirse y no pueden quedar en un estado inválido.

mod celular;
mod correo;
mod dinero;
mod porcentaje;

pub use celular::Celular;
pub use correo::Correo;
pub use dinero::Dinero;
pub use porcentaje::Porcentaje;
