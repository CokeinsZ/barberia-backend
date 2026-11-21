//! Reglas de la venta, sin infraestructura: totales (RN-12), descuentos (RN-13), propina
//! (RN-14), comisión (RN-15) y cuadre de los pagos.

use nucleo::{Dinero, ErrorDeCampo, Porcentaje, Validacion};

/// Máximo de servicios en una venta (posiciones 1 a 20 en `venta_linea`).
pub(crate) const MAX_SERVICIOS: usize = 20;
/// Tope de cordura para la propina (un error de digitación no debe pasar).
pub(crate) const MAX_PROPINA: i64 = 1_000_000;

/// Tipo de descuento predefinido (`ventas.tipo_descuento`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoDescuento {
    Porcentaje(Porcentaje),
    ValorFijo(Dinero),
}

/// Totales de una venta. Los calcula siempre el servidor: los del cliente no se usan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Totales {
    pub(crate) subtotal: Dinero,
    pub(crate) descuento: Dinero,
    /// Subtotal − descuento: ingreso de la barbería y base de la comisión.
    pub(crate) venta_neta: Dinero,
    pub(crate) propina: Dinero,
    /// Venta neta + propina: lo que paga el cliente.
    pub(crate) total: Dinero,
    pub(crate) comision: Dinero,
}

/// RN-12 a RN-15. El descuento nunca supera el subtotal; la propina no paga comisión.
pub(crate) fn calcular(
    precios: &[Dinero],
    descuento: Option<TipoDescuento>,
    propina: Dinero,
    comision: Porcentaje,
) -> Result<Totales, Vec<ErrorDeCampo>> {
    let mut v = Validacion::default();
    let subtotal = precios
        .iter()
        .try_fold(Dinero::CERO, |suma, p| suma.sumar(*p))
        .unwrap_or(Dinero::CERO);
    let valor_descuento = match descuento {
        None => Dinero::CERO,
        Some(TipoDescuento::Porcentaje(p)) => p.aplicar(subtotal),
        Some(TipoDescuento::ValorFijo(valor)) => {
            if valor > subtotal {
                v.error("descuento", "El descuento supera el valor de los servicios.");
            }
            valor
        }
    };
    if propina.valor() > MAX_PROPINA {
        v.error("propina", "Revisa la propina: parece demasiado alta.");
    }
    let venta_neta = subtotal.restar(valor_descuento).unwrap_or(Dinero::CERO);
    let total = venta_neta.sumar(propina).unwrap_or(venta_neta);
    v.terminar(|| Totales {
        subtotal,
        descuento: valor_descuento,
        venta_neta,
        propina,
        total,
        comision: comision.aplicar(venta_neta),
    })
}

/// Los pagos cuadran con el total: métodos distintos, montos positivos y la suma exacta (si el
/// total es cero, no hay pagos).
pub(crate) fn validar_pagos(pagos: &[(i16, Dinero)], total: Dinero) -> Result<(), Vec<ErrorDeCampo>> {
    let mut v = Validacion::default();
    let mut metodos: Vec<i16> = pagos.iter().map(|(m, _)| *m).collect();
    metodos.sort_unstable();
    metodos.dedup();
    if metodos.len() != pagos.len() {
        v.error("pagos", "No repitas el método de pago.");
    }
    if pagos.iter().any(|(_, monto)| monto.valor() <= 0) {
        v.error("pagos", "Cada pago debe ser mayor que cero.");
    }
    if total.valor() > 0 && pagos.is_empty() {
        v.error("pagos", "Indica el método de pago.");
    }
    let suma = pagos.iter().map(|(_, m)| m.valor()).sum::<i64>();
    if suma != total.valor() {
        v.error(
            "pagos",
            format!(
                "Los pagos suman {} y el total a cobrar es {}.",
                Dinero::pesos(suma).unwrap_or(Dinero::CERO),
                total
            ),
        );
    }
    v.terminar(|| ())
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn pesos(valor: i64) -> Dinero {
        Dinero::pesos(valor).unwrap()
    }

    #[test]
    fn e2e_ven_01_corte_cejas_diez_por_ciento_y_propina() {
        // Corte $35.000 + cejas $10.000, 10 %, propina $3.500, comisión 50 %.
        let t = calcular(
            &[pesos(35_000), pesos(10_000)],
            Some(TipoDescuento::Porcentaje(Porcentaje::entero(10).unwrap())),
            pesos(3_500),
            Porcentaje::entero(50).unwrap(),
        )
        .unwrap();
        assert_eq!(
            (
                t.subtotal,
                t.descuento,
                t.venta_neta,
                t.propina,
                t.total,
                t.comision
            ),
            (
                pesos(45_000),
                pesos(4_500),
                pesos(40_500),
                pesos(3_500),
                pesos(44_000),
                pesos(20_250)
            )
        );
    }

    #[test]
    fn la_comision_se_redondea_al_peso_y_la_propina_no_la_cambia() {
        let t = calcular(
            &[pesos(33_333)],
            None,
            pesos(10_000),
            Porcentaje::desde_centesimas(4_550).unwrap(),
        )
        .unwrap();
        assert_eq!(t.comision, pesos(15_167), "33.333 × 45,5 % = 15.166,5 → 15.167");
        assert_eq!(t.total, pesos(43_333));
    }

    #[test]
    fn el_descuento_fijo_no_supera_el_subtotal() {
        let error = calcular(
            &[pesos(4_000)],
            Some(TipoDescuento::ValorFijo(pesos(5_000))),
            Dinero::CERO,
            Porcentaje::entero(50).unwrap(),
        )
        .unwrap_err();
        assert_eq!(error[0].campo, "descuento");
        let exacto = calcular(
            &[pesos(5_000)],
            Some(TipoDescuento::ValorFijo(pesos(5_000))),
            Dinero::CERO,
            Porcentaje::entero(50).unwrap(),
        )
        .unwrap();
        assert_eq!(exacto.total, Dinero::CERO);
    }

    #[test]
    fn los_pagos_deben_cuadrar() {
        let total = pesos(44_000);
        assert!(validar_pagos(&[(2, pesos(44_000))], total).is_ok());
        assert!(
            validar_pagos(&[(1, pesos(20_000)), (2, pesos(24_000))], total).is_ok(),
            "pago combinado (RF-VEN-13)"
        );
        assert!(validar_pagos(&[(2, pesos(40_000))], total).is_err());
        assert!(
            validar_pagos(&[(2, pesos(22_000)), (2, pesos(22_000))], total).is_err(),
            "método repetido"
        );
        assert!(validar_pagos(&[], total).is_err());
        assert!(
            validar_pagos(&[], Dinero::CERO).is_ok(),
            "cortesía total: sin pagos"
        );
        assert!(validar_pagos(&[(1, Dinero::CERO)], Dinero::CERO).is_err());
    }
}
