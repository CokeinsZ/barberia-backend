//! Indicadores (RN-21): ticket promedio, porcentajes y ocupación de los barberos.

use chrono::{NaiveTime, Timelike};

/// Ventas netas ÷ clientes atendidos, al peso (las mitades hacia arriba). `None` sin clientes.
pub(crate) fn ticket_promedio(ventas_netas: i64, clientes: i64) -> Option<i64> {
    (clientes > 0).then(|| (ventas_netas + clientes / 2) / clientes)
}

/// Porcentaje con un decimal (`37.5`). `None` si el total es cero.
pub(crate) fn porcentaje(parte: i64, total: i64) -> Option<f64> {
    if total <= 0 {
        return None;
    }
    // Décimas de punto con redondeo entero, las mitades lejos del cero (la división entera
    // trunca hacia el cero: se redondea la magnitud y luego se pone el signo).
    let magnitud = (parte.abs() * 1000 + total / 2) / total;
    let decimas = if parte < 0 { -magnitud } else { magnitud };
    #[allow(clippy::cast_precision_loss)]
    Some(decimas as f64 / 10.0)
}

/// Promedio con un decimal (`7 / 4` → `1.8`). Cero si no hay con qué dividir.
pub(crate) fn promedio(total: i64, partes: i64) -> f64 {
    if partes <= 0 {
        return 0.0;
    }
    let decimas = (total * 10 + partes / 2) / partes;
    #[allow(clippy::cast_precision_loss)]
    {
        decimas as f64 / 10.0
    }
}

/// Variación porcentual frente a una referencia (RF-DAS-02). `None` si la referencia es cero.
pub(crate) fn variacion(actual: i64, referencia: i64) -> Option<f64> {
    porcentaje(actual - referencia, referencia)
}

/// Tipo de día para el horario de un barbero (las mismas reglas del agendamiento, RN-03).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TipoDia {
    Normal,
    /// Usa las franjas del domingo.
    Festivo,
    Cerrado,
    CierreTemprano(NaiveTime),
}

/// Minutos desde la medianoche de un intervalo del día.
pub(crate) type Intervalo = (u32, u32);

pub(crate) fn minutos(hora: NaiveTime) -> u32 {
    hora.hour() * 60 + hora.minute()
}

/// Franjas del día según su tipo: festivo → las del domingo; cerrado → ninguna; cierre
/// temprano → recortadas a esa hora. `franjas` son `(día ISO, inicio, fin)`.
pub(crate) fn franjas_del_dia(
    franjas: &[(u8, NaiveTime, NaiveTime)],
    dia_semana: u8,
    tipo: TipoDia,
) -> Vec<Intervalo> {
    let dia = match tipo {
        TipoDia::Festivo => 7,
        _ => dia_semana,
    };
    let tope = match tipo {
        TipoDia::Cerrado => return Vec::new(),
        TipoDia::CierreTemprano(hora) => minutos(hora),
        _ => 24 * 60,
    };
    franjas
        .iter()
        .filter(|(d, _, _)| *d == dia)
        .map(|(_, inicio, fin)| (minutos(*inicio), minutos(*fin).min(tope)))
        .filter(|(inicio, fin)| fin > inicio)
        .collect()
}

fn traslape(a: Intervalo, b: Intervalo) -> Option<Intervalo> {
    let (inicio, fin) = (a.0.max(b.0), a.1.min(b.1));
    (fin > inicio).then_some((inicio, fin))
}

fn dentro(intervalo: Intervalo, franjas: &[Intervalo]) -> u32 {
    franjas
        .iter()
        .filter_map(|f| traslape(intervalo, *f))
        .map(|(inicio, fin)| fin - inicio)
        .sum()
}

/// Ocupación del día (RN-21): minutos con citas ÷ minutos disponibles, descontando los
/// bloqueos. Devuelve `(ocupados, disponibles)`. Lo que una cita tiene dentro de un bloqueo (se
/// bloqueó después de agendarla) no cuenta como ocupado: esos minutos ya no eran disponibles.
/// Las mismas reglas que el tiempo libre de «Mis citas» (`agenda::dominio::resumen`).
pub(crate) fn ocupacion(franjas: &[Intervalo], bloqueos: &[Intervalo], citas: &[Intervalo]) -> (u32, u32) {
    let total: u32 = franjas.iter().map(|(i, f)| f - i).sum();
    let bloqueado: u32 = bloqueos.iter().map(|b| dentro(*b, franjas)).sum();
    let disponibles = total.saturating_sub(bloqueado);
    let ocupados: u32 = citas
        .iter()
        .map(|c| {
            let en_bloqueo: u32 = bloqueos
                .iter()
                .filter_map(|b| traslape(*c, *b))
                .map(|t| dentro(t, franjas))
                .sum();
            dentro(*c, franjas).saturating_sub(en_bloqueo)
        })
        .sum();
    (ocupados.min(disponibles), disponibles)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn h(texto: &str) -> NaiveTime {
        NaiveTime::parse_from_str(texto, "%H:%M").unwrap()
    }

    #[test]
    fn ticket_y_porcentajes() {
        assert_eq!(ticket_promedio(100_000, 3), Some(33_333));
        assert_eq!(ticket_promedio(100_001, 2), Some(50_001), "las mitades suben");
        assert_eq!(ticket_promedio(0, 0), None);
        assert_eq!(porcentaje(1, 3), Some(33.3));
        assert_eq!(porcentaje(2, 3), Some(66.7));
        assert_eq!(porcentaje(5, 0), None);
        assert_eq!(variacion(120, 100), Some(20.0));
        assert_eq!(variacion(80, 100), Some(-20.0));
        assert_eq!(variacion(0, 100), Some(-100.0));
        assert_eq!(porcentaje(-1, 3), Some(-33.3));
        assert_eq!(
            porcentaje(-2, 3),
            Some(-66.7),
            "simétrico: las mitades se alejan del cero"
        );
        assert_eq!(variacion(80, 0), None);
        assert_eq!(promedio(7, 4), 1.8);
        assert_eq!(promedio(2, 4), 0.5);
        assert_eq!(promedio(3, 0), 0.0);
    }

    #[test]
    fn la_ocupacion_descuenta_bloqueos_y_respeta_el_tipo_de_dia() {
        let franjas = [
            (1, h("09:00"), h("13:00")),
            (1, h("14:00"), h("19:00")),
            (7, h("10:00"), h("14:00")),
        ];
        let lunes = franjas_del_dia(&franjas, 1, TipoDia::Normal);
        assert_eq!(lunes, [(540, 780), (840, 1140)]);
        assert_eq!(
            franjas_del_dia(&franjas, 1, TipoDia::Festivo),
            [(600, 840)],
            "festivo: horario de domingo"
        );
        assert!(franjas_del_dia(&franjas, 1, TipoDia::Cerrado).is_empty());
        assert_eq!(
            franjas_del_dia(&franjas, 1, TipoDia::CierreTemprano(h("15:00"))),
            [(540, 780), (840, 900)]
        );
        // 9 h disponibles − 1 h de bloqueo = 8 h; 2 h de citas (una cruza el almuerzo: solo cuenta lo que cae en franja).
        let bloqueos = [(minutos(h("16:00")), minutos(h("17:00")))];
        let citas = [
            (minutos(h("09:00")), minutos(h("10:00"))),
            (minutos(h("12:30")), minutos(h("14:30"))),
        ];
        assert_eq!(ocupacion(&lunes, &bloqueos, &citas), (120, 480));
        assert_eq!(ocupacion(&[], &[], &citas), (0, 0), "ausente o cerrado");
        // Una cita que quedó dentro del bloqueo (se bloqueó después): no suma.
        let con_cita_bloqueada = [citas[0], citas[1], (minutos(h("16:00")), minutos(h("16:30")))];
        assert_eq!(ocupacion(&lunes, &bloqueos, &con_cita_bloqueada), (120, 480));
    }
}
