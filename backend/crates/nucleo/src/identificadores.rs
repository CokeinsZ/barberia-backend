//! Identificadores tipados (patrón *newtype*): el compilador impide pasar el id de un
//! cliente donde se espera el de un barbero.

/// Declara un identificador entero tipado. Los módulos pueden usarlo para sus propios ids.
#[macro_export]
macro_rules! identificador {
    ($(#[$meta:meta])* $nombre:ident) => {
        $(#[$meta])*
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
            $crate::__serde::Serialize, $crate::__serde::Deserialize,
        )]
        #[serde(crate = "::nucleo::__serde", transparent)]
        pub struct $nombre(pub i64);

        impl ::std::fmt::Display for $nombre {
            fn fmt(&self, f: &mut ::std::fmt::Formatter<'_>) -> ::std::fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<i64> for $nombre {
            fn from(valor: i64) -> Self {
                Self(valor)
            }
        }
    };
}

identificador!(
    /// Cuenta del personal (administrador o barbero).
    IdUsuario
);
identificador!(IdBarbero);
identificador!(IdCliente);
identificador!(IdServicio);
identificador!(IdCita);
identificador!(IdVenta);
identificador!(IdArchivo);
identificador!(IdDiaEspecial);
