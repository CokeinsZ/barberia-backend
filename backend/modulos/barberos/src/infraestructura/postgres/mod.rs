//! Repositorios del esquema `barberos` y adaptadores de los puertos que definen otros módulos.

mod barberos;
mod expediente;
mod novedades;
mod oferentes;
mod perfiles;

pub(crate) use barberos::RepositorioBarberosPg;
pub(crate) use expediente::RepositorioExpedientePg;
pub(crate) use novedades::RepositorioNovedadesPg;
pub(crate) use oferentes::OferentesServicioPg;
pub(crate) use perfiles::PerfilesBarberosPg;
