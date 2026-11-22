//! HTML de los reportes en PDF (RF-REP-06): A4, encabezado con la marca, el tipo de reporte, el
//! periodo y la fecha de generación, y pie con el NIT y la numeración de páginas. Todo dato que
//! viene de la base se escapa.

use chrono::DateTime;
use chrono_tz::Tz;

use super::dto::{ReporteBarberoDto, ReporteClientesDto, ResumenDto, TotalesDto};
use crate::dominio::formato::{decimal, fecha_corta, fecha_larga, hora, numero, pesos};

const ESTILOS: &str = "
@page { size: A4; }
* { box-sizing: border-box; }
body { font-family: 'Helvetica Neue', Arial, sans-serif; font-size: 10.5pt; color: #1a1712; margin: 0;
       -webkit-print-color-adjust: exact; print-color-adjust: exact; }
h1 { font-size: 17pt; margin: 0 0 4px; color: #15120d; }
h2 { font-size: 12.5pt; margin: 18px 0 6px; color: #8a6a2e; border-bottom: 1px solid #e6dcc8; padding-bottom: 3px; }
.periodo { color: #6e675b; margin: 0 0 12px; }
.tarjetas { display: grid; grid-template-columns: repeat(4, 1fr); gap: 6px; margin: 8px 0 4px; }
.tarjeta { border: 1px solid #e6dcc8; border-radius: 4px; padding: 6px 8px; background: #fbf8f2; }
.tarjeta .etiqueta { font-size: 8pt; color: #6e675b; text-transform: uppercase; letter-spacing: .03em; }
.tarjeta .valor { font-size: 12.5pt; font-weight: bold; margin-top: 2px; }
table { width: 100%; border-collapse: collapse; margin: 4px 0 8px; }
th { text-align: left; font-size: 8.5pt; color: #6e675b; text-transform: uppercase; border-bottom: 1.5px solid #c9a35a; padding: 4px 6px; }
td { padding: 4px 6px; border-bottom: 1px solid #eee7da; vertical-align: top; }
tr:nth-child(even) td { background: #fcfaf6; }
.num { text-align: right; white-space: nowrap; }
tfoot td { font-weight: bold; border-top: 1.5px solid #c9a35a; background: #fff !important; }
.vacio { color: #6e675b; font-style: italic; }
.liquidacion td { font-size: 11pt; }
.liquidacion tr:last-child td { font-weight: bold; font-size: 12pt; }
.seccion { page-break-before: always; }
.seccion:first-of-type { page-break-before: auto; }
.nota { font-size: 8.5pt; color: #6e675b; }
.nw { white-space: nowrap; }
";

pub(crate) fn escapar(texto: &str) -> String {
    texto
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn pagina(cuerpo: &str) -> String {
    format!(
        "<!doctype html><html lang=\"es\"><head><meta charset=\"utf-8\"><title>Reporte</title><style>{ESTILOS}</style></head><body>{cuerpo}</body></html>"
    )
}

/// Estilo del encabezado y del pie. Chromium inserta cada plantilla dentro de su propio
/// contenedor flexible (`#header`, `#footer`): los estilos van en un `div` de ancho completo,
/// no en `body` (que sería el de la página de plantillas).
const ESTILO_MARGEN: &str = "<style>.margen { width: 100%; box-sizing: border-box; padding: 0 14mm; display: flex; \
     justify-content: space-between; align-items: baseline; gap: 8mm; font-family: Arial, sans-serif; font-size: 7.5pt; \
     color: #6e675b; -webkit-print-color-adjust: exact; } .margen b { color: #15120d; } \
     .margen .izquierda { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; } \
     .margen .derecha { flex: none; white-space: nowrap; }</style>";

/// Encabezado de cada página (Chromium lo dibuja en el margen superior).
pub(crate) fn encabezado(negocio: &str, titulo: &str, periodo: &str, generado: DateTime<Tz>) -> String {
    format!(
        "{ESTILO_MARGEN}<div class=\"margen\"><span class=\"izquierda\"><b>{negocio}</b> · {titulo} · {periodo}</span>\
         <span class=\"derecha\">Generado el {fecha} a las {hora}</span></div>",
        negocio = escapar(negocio),
        titulo = escapar(titulo),
        periodo = escapar(periodo),
        fecha = fecha_corta(generado.date_naive()),
        hora = hora(generado.time()),
    )
}

/// Pie de cada página: NIT y «Página X de Y» (Chromium llena `pageNumber` y `totalPages`).
pub(crate) fn pie(negocio: &str, nit: Option<&str>) -> String {
    let nit = nit.map_or_else(
        || "NIT sin registrar".to_owned(),
        |n| format!("NIT {}", escapar(n)),
    );
    format!(
        "{ESTILO_MARGEN}<div class=\"margen\"><span class=\"izquierda\">{negocio} · {nit}</span>\
         <span class=\"derecha\">Página&nbsp;<span class=\"pageNumber\"></span>&nbsp;de&nbsp;<span class=\"totalPages\"></span></span></div>",
        negocio = escapar(negocio),
    )
}

fn tarjeta(etiqueta: &str, valor: &str) -> String {
    format!(
        "<div class=\"tarjeta\"><div class=\"etiqueta\">{}</div><div class=\"valor\">{}</div></div>",
        escapar(etiqueta),
        escapar(valor)
    )
}

fn tarjetas_totales(t: &TotalesDto) -> String {
    let ticket = t.ticket_promedio.map_or_else(|| "—".to_owned(), pesos);
    format!(
        "<div class=\"tarjetas\">{}{}{}{}{}{}{}{}</div>",
        tarjeta("Ventas netas", &pesos(t.ventas_netas)),
        tarjeta("Clientes atendidos", &numero(t.clientes_atendidos)),
        tarjeta("Servicios", &numero(t.servicios)),
        tarjeta("Ticket promedio", &ticket),
        tarjeta("Descuentos", &pesos(t.descuentos)),
        tarjeta("Propinas", &pesos(t.propinas)),
        tarjeta("Total cobrado", &pesos(t.total_cobrado)),
        tarjeta("Comisiones", &pesos(t.comisiones)),
    )
}

/// Tabla con encabezados (las columnas con `#` se alinean a la derecha) y filas ya escapadas.
fn tabla(columnas: &[&str], filas: &[Vec<String>], pie: Option<Vec<String>>) -> String {
    if filas.is_empty() {
        return "<p class=\"vacio\">Sin movimientos en el periodo.</p>".to_owned();
    }
    let numericas: Vec<bool> = columnas.iter().map(|c| c.starts_with('#')).collect();
    let celda = |i: usize, valor: &str, etiqueta: &str| {
        if numericas[i] {
            format!("<{etiqueta} class=\"num\">{valor}</{etiqueta}>")
        } else {
            format!("<{etiqueta}>{valor}</{etiqueta}>")
        }
    };
    let cabecera: String = columnas
        .iter()
        .enumerate()
        .map(|(i, c)| celda(i, &escapar(c.trim_start_matches('#')), "th"))
        .collect();
    let cuerpo: String = filas
        .iter()
        .map(|f| {
            format!(
                "<tr>{}</tr>",
                f.iter()
                    .enumerate()
                    .map(|(i, v)| celda(i, v, "td"))
                    .collect::<String>()
            )
        })
        .collect();
    let pie = pie.map_or_else(String::new, |f| {
        format!(
            "<tfoot><tr>{}</tr></tfoot>",
            f.iter()
                .enumerate()
                .map(|(i, v)| celda(i, v, "td"))
                .collect::<String>()
        )
    });
    format!("<table><thead><tr>{cabecera}</tr></thead><tbody>{cuerpo}</tbody>{pie}</table>")
}

pub(crate) fn resumen(r: &ResumenDto) -> String {
    let semanas: Vec<Vec<String>> = r
        .por_semana
        .iter()
        .map(|s| {
            vec![
                format!("{} al {}", fecha_corta(s.desde), fecha_corta(s.hasta)),
                pesos(s.ventas_netas),
            ]
        })
        .collect();
    let servicios: Vec<Vec<String>> = r
        .por_servicio
        .iter()
        .map(|s| {
            vec![
                escapar(&s.nombre),
                escapar(&s.categoria),
                numero(s.unidades),
                pesos(s.ingresos),
            ]
        })
        .collect();
    let metodos: Vec<Vec<String>> = r
        .por_metodo
        .iter()
        .map(|m| vec![escapar(&m.nombre), pesos(m.total)])
        .collect();
    let barberos: Vec<Vec<String>> = r
        .por_barbero
        .iter()
        .map(|b| {
            vec![
                escapar(&b.nombre),
                numero(b.servicios),
                pesos(b.ventas_netas),
                pesos(b.comision),
                pesos(b.propinas),
                pesos(b.total_a_pagar),
            ]
        })
        .collect();
    let total_barberos = vec![
        "Total".to_owned(),
        numero(r.por_barbero.iter().map(|b| b.servicios).sum::<i64>()),
        pesos(r.por_barbero.iter().map(|b| b.ventas_netas).sum()),
        pesos(r.por_barbero.iter().map(|b| b.comision).sum()),
        pesos(r.por_barbero.iter().map(|b| b.propinas).sum()),
        pesos(r.por_barbero.iter().map(|b| b.total_a_pagar).sum()),
    ];
    pagina(&format!(
        "<h1>Resumen de ventas y servicios</h1><p class=\"periodo\">{periodo}</p>{tarjetas}\
         <h2>Ventas netas por semana</h2>{semanas}\
         <h2>Por servicio</h2>{servicios}<p class=\"nota\">Ingresos al precio de cada servicio, antes de descuentos.</p>\
         <h2>Por método de pago</h2>{metodos}<p class=\"nota\">Incluye las propinas, que son del barbero.</p>\
         <h2>Por barbero</h2>{barberos}",
        periodo = escapar(&r.periodo.texto),
        tarjetas = tarjetas_totales(&r.totales),
        semanas = tabla(&["Semana", "#Ventas netas"], &semanas, None),
        servicios = tabla(
            &["Servicio", "Categoría", "#Unidades", "#Ingresos"],
            &servicios,
            None
        ),
        metodos = tabla(&["Método", "#Total cobrado"], &metodos, None),
        barberos = tabla(
            &[
                "Barbero",
                "#Servicios",
                "#Ventas netas",
                "#Comisión",
                "#Propinas",
                "#Total a pagar"
            ],
            &barberos,
            (!r.por_barbero.is_empty()).then_some(total_barberos),
        ),
    ))
}

/// Uno o varios barberos, cada uno en su sección (RF-REP-03 y 04).
pub(crate) fn barberos(reportes: &[ReporteBarberoDto]) -> String {
    if reportes.is_empty() {
        return pagina(
            "<h1>Reporte detallado de los barberos</h1><p class=\"vacio\">No hay barberos en el periodo.</p>",
        );
    }
    let secciones: String = reportes.iter().map(seccion_barbero).collect();
    pagina(&secciones)
}

/// Etiqueta del tipo de vinculación (enum `barberos.tipo_vinculacion`).
fn vinculacion(codigo: &str) -> String {
    match codigo {
        "prestacion_servicios" => "Prestación de servicios".to_owned(),
        "laboral_indefinido" => "Contrato laboral a término indefinido".to_owned(),
        "laboral_termino_fijo" => "Contrato laboral a término fijo".to_owned(),
        otro => otro.replace('_', " "),
    }
}

fn seccion_barbero(r: &ReporteBarberoDto) -> String {
    let b = &r.barbero;
    let vinculacion = vinculacion(&b.tipo_vinculacion);
    let retiro = b
        .fecha_retiro
        .map_or_else(String::new, |f| format!(" · Retirado el {}", fecha_corta(f)));
    let servicios: Vec<Vec<String>> = r
        .servicios
        .iter()
        .map(|s| vec![escapar(&s.nombre), numero(s.unidades), pesos(s.ingresos)])
        .collect();
    let ventas: Vec<Vec<String>> = r
        .ventas
        .iter()
        .map(|v| {
            let local = v.registrada_en;
            vec![
                format!(
                    "<span class=\"nw\">{}</span><br><span class=\"nw nota\">{}</span>",
                    fecha_corta(local.date_naive()),
                    hora(local.time())
                ),
                v.numero_ticket.to_string(),
                escapar(&v.cliente),
                escapar(&v.servicios),
                pesos(v.ventas_netas),
                pesos(v.propina),
                pesos(v.comision),
            ]
        })
        .collect();
    let l = &r.liquidacion;
    format!(
        "<div class=\"seccion\"><h1>{nombre}</h1>\
         <p class=\"periodo\">{periodo} · {tipo_doc} {documento} · {vinculacion} desde el {ingreso} · Comisión vigente: {comision} %{retiro}</p>\
         {tarjetas}\
         <h2>Liquidación</h2>\
         <table class=\"liquidacion\"><tbody>\
         <tr><td>Ventas netas</td><td class=\"num\">{netas}</td></tr>\
         <tr><td>Comisión (cada venta con su porcentaje)</td><td class=\"num\">{comision_valor}</td></tr>\
         <tr><td>Propinas (todas, sin importar el método de pago)</td><td class=\"num\">{propinas}</td></tr>\
         <tr><td>Total a pagar</td><td class=\"num\">{total}</td></tr>\
         </tbody></table>\
         <h2>Servicios realizados</h2>{servicios}\
         <h2>Detalle de las ventas</h2>{ventas}</div>",
        nombre = escapar(&b.nombre),
        periodo = escapar(&r.periodo.texto),
        tipo_doc = escapar(&b.tipo_documento),
        documento = escapar(&b.numero_documento),
        vinculacion = escapar(&vinculacion),
        ingreso = fecha_larga(b.fecha_ingreso),
        comision = decimal(b.porcentaje_comision),
        tarjetas = tarjetas_totales(&r.totales),
        netas = pesos(l.ventas_netas),
        comision_valor = pesos(l.comision),
        propinas = pesos(l.propinas),
        total = pesos(l.total_a_pagar),
        servicios = tabla(&["Servicio", "#Unidades", "#Ingresos"], &servicios, None),
        ventas = tabla(
            &[
                "Fecha",
                "#Ticket",
                "Cliente",
                "Servicios",
                "#Venta neta",
                "#Propina",
                "#Comisión"
            ],
            &ventas,
            None
        ),
    )
}

pub(crate) fn clientes(r: &ReporteClientesDto) -> String {
    let c = &r.clientes;
    let retorno = c
        .porcentaje_retorno
        .map_or_else(|| "—".to_owned(), |p| format!("{} %", decimal(p)));
    let destacados: Vec<Vec<String>> = r
        .mas_visitas
        .iter()
        .map(|d| {
            vec![
                escapar(&d.nombre),
                escapar(d.celular.as_deref().unwrap_or("—")),
                numero(d.visitas),
                pesos(d.ventas_netas),
            ]
        })
        .collect();
    let reactivar: Vec<Vec<String>> = r
        .por_reactivar
        .clientes
        .iter()
        .map(|d| {
            vec![
                escapar(&d.nombre),
                escapar(&d.celular),
                fecha_corta(d.ultima_visita),
                numero(d.dias_sin_volver),
                escapar(d.ultimo_servicio.as_deref().unwrap_or("—")),
            ]
        })
        .collect();
    pagina(&format!(
        "<h1>Reporte de clientes</h1><p class=\"periodo\">{periodo}</p>\
         <div class=\"tarjetas\">{}{}{}{}{}{}{}</div>\
         <h2>Clientes con más visitas</h2>{destacados}\
         <h2>Por reactivar (más de {dias} días sin volver: {total})</h2>{reactivar}",
        tarjeta("Atendidos", &numero(c.atendidos)),
        tarjeta("Nuevos", &numero(c.nuevos)),
        tarjeta("Recurrentes", &numero(c.recurrentes)),
        tarjeta("Retorno (60 días)", &retorno),
        tarjeta("Ocasionales", &numero(c.ocasionales)),
        tarjeta("Inasistencias", &numero(r.inasistencias)),
        tarjeta("Por reactivar", &numero(r.por_reactivar.total)),
        periodo = escapar(&r.periodo.texto),
        destacados = tabla(
            &["Cliente", "Celular", "#Visitas", "#Ventas netas"],
            &destacados,
            None
        ),
        dias = r.por_reactivar.dias_inactividad,
        total = numero(r.por_reactivar.total),
        reactivar = tabla(
            &["Cliente", "Celular", "Última visita", "#Días", "Último servicio"],
            &reactivar,
            None
        ),
    ))
}

#[cfg(test)]
mod pruebas {
    use chrono::NaiveDate;

    use super::*;
    use crate::aplicacion::dto::{PeriodoDto, SemanaDto, ServicioVendidoDto};

    #[test]
    fn el_resumen_escapa_los_datos_y_lleva_las_secciones() {
        let r = ResumenDto {
            periodo: PeriodoDto {
                desde: NaiveDate::from_ymd_opt(2026, 11, 1).unwrap(),
                hasta: NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(),
                texto: "1 al 21 de noviembre de 2026".into(),
                dias: 21,
            },
            totales: TotalesDto {
                clientes_atendidos: 2,
                ventas_netas: 70_000,
                ticket_promedio: Some(35_000),
                ..Default::default()
            },
            por_semana: vec![SemanaDto {
                desde: NaiveDate::from_ymd_opt(2026, 11, 16).unwrap(),
                hasta: NaiveDate::from_ymd_opt(2026, 11, 21).unwrap(),
                ventas_netas: 70_000,
            }],
            por_servicio: vec![ServicioVendidoDto {
                servicio_id: 1,
                nombre: "Corte <script>".into(),
                categoria: "Cabello".into(),
                unidades: 2,
                ingresos: 70_000,
            }],
            por_metodo: vec![],
            por_barbero: vec![],
        };
        let html = resumen(&r);
        assert!(html.contains("1 al 21 de noviembre de 2026"));
        assert!(html.contains("$70.000") && html.contains("$35.000"));
        assert!(html.contains("Corte &lt;script&gt;") && !html.contains("<script>"));
        assert!(
            html.contains("Sin movimientos en el periodo."),
            "sin métodos de pago"
        );
    }

    #[test]
    fn encabezado_y_pie_llevan_marca_periodo_nit_y_paginas() {
        let generado = chrono::Utc::now().with_timezone(&nucleo::ZONA_HORARIA);
        let e = encabezado("Diamante Barbería", "Resumen", "noviembre & más", generado);
        assert!(
            e.contains("Diamante Barbería") && e.contains("noviembre &amp; más") && e.contains("Generado el")
        );
        let p = pie("Diamante Barbería", Some("900.123.456-7"));
        assert!(p.contains("NIT 900.123.456-7") && p.contains("pageNumber") && p.contains("totalPages"));
        assert!(pie("Diamante", None).contains("NIT sin registrar"));
    }
}
