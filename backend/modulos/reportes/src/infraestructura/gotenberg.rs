//! Gotenberg (HTML → PDF con Chromium), en la red interna de Docker Compose.

use std::time::Duration;

use async_trait::async_trait;
use reqwest::multipart::{Form, Part};

use crate::aplicacion::puertos::{DocumentoPdf, ErrorPdf, GeneradorPdf};

/// Un mes de ventas cabe holgado; el de todos los barberos tiene 30 s de meta (RNF-REN-04).
const ESPERA: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub(crate) struct Gotenberg {
    cliente: reqwest::Client,
    url: String,
}

impl Gotenberg {
    pub(crate) fn nuevo(url_base: &str) -> Result<Self, ErrorPdf> {
        let cliente = reqwest::Client::builder()
            .timeout(ESPERA)
            .build()
            .map_err(|e| ErrorPdf(format!("cliente HTTP: {e}")))?;
        Ok(Self {
            cliente,
            url: format!("{}/forms/chromium/convert/html", url_base.trim_end_matches('/')),
        })
    }
}

fn html(nombre: &str, contenido: &str) -> Result<Part, ErrorPdf> {
    Part::text(contenido.to_owned())
        .file_name(nombre.to_owned())
        .mime_str("text/html; charset=utf-8")
        .map_err(|e| ErrorPdf(e.to_string()))
}

#[async_trait]
impl GeneradorPdf for Gotenberg {
    async fn generar(&self, documento: &DocumentoPdf) -> Result<Vec<u8>, ErrorPdf> {
        // A4 en pulgadas; el encabezado y el pie se dibujan dentro de los márgenes.
        let formulario = Form::new()
            .part("files", html("index.html", &documento.cuerpo)?)
            .part("files", html("header.html", &documento.encabezado)?)
            .part("files", html("footer.html", &documento.pie)?)
            .text("paperWidth", "8.27")
            .text("paperHeight", "11.7")
            .text("marginTop", "0.85")
            .text("marginBottom", "0.75")
            .text("marginLeft", "0.55")
            .text("marginRight", "0.55")
            .text("printBackground", "true");
        let respuesta = self
            .cliente
            .post(&self.url)
            .multipart(formulario)
            .send()
            .await
            .map_err(|e| ErrorPdf(format!("Gotenberg no responde: {e}")))?;
        let estado = respuesta.status();
        let bytes = respuesta
            .bytes()
            .await
            .map_err(|e| ErrorPdf(format!("respuesta incompleta de Gotenberg: {e}")))?;
        if !estado.is_success() {
            return Err(ErrorPdf(format!(
                "Gotenberg respondió {estado}: {}",
                String::from_utf8_lossy(&bytes[..bytes.len().min(300)])
            )));
        }
        if !bytes.starts_with(b"%PDF") {
            return Err(ErrorPdf("Gotenberg no devolvió un PDF".into()));
        }
        Ok(bytes.to_vec())
    }
}

#[cfg(test)]
mod pruebas {
    //! Con `PRUEBAS_GOTENBERG_URL` (p. ej. `http://127.0.0.1:3999`) genera PDF reales con el
    //! volumen de un mes intenso y mide los tiempos de RNF-REN-04; con `PRUEBAS_PDF_DIR`, además
    //! los guarda para revisarlos con `pdfinfo` y `pdftotext`. Sin la variable, no hace nada.

    use std::time::Instant;

    use chrono::{NaiveDate, TimeZone};
    use nucleo::ZONA_HORARIA;

    use super::*;
    use crate::aplicacion::dto::{
        BarberoDto, ClienteDestacadoDto, ClienteReactivarDto, ClientesPeriodoDto, LiquidacionDto,
        MetodoPagoDto, PeriodoDto, PorReactivarDto, ReporteBarberoDto, ReporteClientesDto, ResumenDto,
        SemanaDto, ServicioVendidoDto, TotalesDto, VentaDetalleDto,
    };
    use crate::aplicacion::pdf;
    use crate::dominio::periodo::Periodo;

    fn fecha(d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 11, d).unwrap()
    }

    fn periodo() -> PeriodoDto {
        Periodo {
            desde: fecha(1),
            hasta: fecha(30),
        }
        .into()
    }

    fn servicios() -> Vec<ServicioVendidoDto> {
        [
            ("Corte clásico", 35_000, 180),
            ("Corte + barba", 50_000, 90),
            ("Diseño de cejas", 10_000, 60),
        ]
        .iter()
        .enumerate()
        .map(|(i, (nombre, precio, unidades))| ServicioVendidoDto {
            servicio_id: i64::try_from(i).unwrap() + 1,
            nombre: (*nombre).into(),
            categoria: "Cabello".into(),
            unidades: *unidades,
            ingresos: precio * unidades,
        })
        .collect()
    }

    /// Un barbero con `ventas` ventas en el mes (10 al día es un mes muy ocupado).
    fn barbero(id: i64, ventas: i64) -> ReporteBarberoDto {
        let detalle: Vec<VentaDetalleDto> = (0..ventas)
            .map(|i| {
                let dia = u32::try_from(i % 30).unwrap() + 1;
                let hora = 9 + u32::try_from(i % 10).unwrap();
                VentaDetalleDto {
                    venta_id: id * 10_000 + i,
                    numero_ticket: id * 10_000 + i,
                    registrada_en: ZONA_HORARIA
                        .with_ymd_and_hms(2026, 11, dia, hora, 15, 0)
                        .unwrap()
                        .fixed_offset(),
                    cliente: if i % 7 == 0 {
                        "Cliente ocasional".into()
                    } else {
                        format!("Cliente Número {i} <&>")
                    },
                    servicios: "Corte clásico, Diseño de cejas".into(),
                    ventas_netas: 45_000,
                    propina: 2_000,
                    comision: 20_250,
                }
            })
            .collect();
        let netas = 45_000 * ventas;
        ReporteBarberoDto {
            periodo: periodo(),
            barbero: BarberoDto {
                id,
                nombre: format!("Barbero {id} Pérez"),
                tipo_documento: "CC".into(),
                numero_documento: format!("10203040{id}"),
                tipo_vinculacion: "prestacion_servicios".into(),
                fecha_ingreso: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
                porcentaje_comision: 45.0,
                fecha_retiro: None,
            },
            totales: TotalesDto {
                clientes_atendidos: ventas,
                servicios: ventas * 2,
                subtotal: netas,
                ventas_netas: netas,
                propinas: 2_000 * ventas,
                total_cobrado: netas + 2_000 * ventas,
                comisiones: 20_250 * ventas,
                ticket_promedio: Some(45_000),
                ..Default::default()
            },
            clientes_distintos: ventas - ventas / 7,
            servicios: servicios(),
            liquidacion: LiquidacionDto {
                barbero_id: id,
                nombre: format!("Barbero {id} Pérez"),
                clientes_atendidos: ventas,
                servicios: ventas * 2,
                ventas_netas: netas,
                comision: 20_250 * ventas,
                propinas: 2_000 * ventas,
                total_a_pagar: 22_250 * ventas,
            },
            ventas: detalle,
        }
    }

    fn resumen() -> ResumenDto {
        let barberos: Vec<ReporteBarberoDto> = (1..=6).map(|id| barbero(id, 300)).collect();
        ResumenDto {
            periodo: periodo(),
            totales: TotalesDto {
                clientes_atendidos: 1_800,
                servicios: 3_600,
                subtotal: 81_000_000,
                ventas_netas: 81_000_000,
                propinas: 3_600_000,
                total_cobrado: 84_600_000,
                comisiones: 36_450_000,
                ticket_promedio: Some(45_000),
                ..Default::default()
            },
            por_semana: Periodo {
                desde: fecha(1),
                hasta: fecha(30),
            }
            .semanas()
            .into_iter()
            .map(|s| SemanaDto {
                desde: s.desde,
                hasta: s.hasta,
                ventas_netas: 18_000_000,
            })
            .collect(),
            por_servicio: servicios(),
            por_metodo: vec![
                MetodoPagoDto {
                    metodo: "efectivo".into(),
                    nombre: "Efectivo".into(),
                    total: 50_000_000,
                },
                MetodoPagoDto {
                    metodo: "nequi".into(),
                    nombre: "Nequi".into(),
                    total: 34_600_000,
                },
            ],
            por_barbero: barberos.iter().map(|b| b.liquidacion.clone()).collect(),
        }
    }

    fn clientes() -> ReporteClientesDto {
        ReporteClientesDto {
            periodo: periodo(),
            clientes: ClientesPeriodoDto {
                atendidos: 900,
                ocasionales: 250,
                nuevos: 120,
                recurrentes: 780,
                con_retorno: 610,
                porcentaje_retorno: Some(67.8),
            },
            inasistencias: 14,
            mas_visitas: (1..=10)
                .map(|i| ClienteDestacadoDto {
                    cliente_id: i,
                    nombre: format!("Cliente Frecuente {i}"),
                    celular: Some(format!("30012345{i:02}")),
                    visitas: 8 - i / 2,
                    ventas_netas: 400_000 - i * 10_000,
                })
                .collect(),
            por_reactivar: PorReactivarDto {
                dias_inactividad: 60,
                total: 140,
                clientes: (1..=100)
                    .map(|i| ClienteReactivarDto {
                        cliente_id: i,
                        nombre: format!("Cliente Inactivo {i}"),
                        celular: format!("31012345{:02}", i % 100),
                        ultima_visita: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                        dias_sin_volver: 111,
                        ultimo_servicio: Some("Corte clásico".into()),
                    })
                    .collect(),
            },
        }
    }

    #[tokio::test]
    async fn genera_pdf_reales_con_gotenberg() {
        let Some(url) = std::env::var("PRUEBAS_GOTENBERG_URL")
            .ok()
            .filter(|u| !u.is_empty())
        else {
            eprintln!("sin PRUEBAS_GOTENBERG_URL: no se generan PDF reales");
            return;
        };
        let gotenberg = Gotenberg::nuevo(&url).unwrap();
        let generado = ZONA_HORARIA.with_ymd_and_hms(2026, 11, 30, 18, 5, 0).unwrap();
        let texto_periodo = periodo().texto;
        let documento = |titulo: &str, cuerpo: String| DocumentoPdf {
            cuerpo,
            encabezado: pdf::encabezado("Diamante Barbería", titulo, &texto_periodo, generado),
            pie: pdf::pie("Diamante Barbería", Some("900.123.456-7")),
        };
        let casos = [
            (
                "resumen",
                documento("Resumen de ventas y servicios", pdf::resumen(&resumen())),
                10,
            ),
            (
                "barbero",
                documento("Reporte detallado del barbero", pdf::barberos(&[barbero(1, 300)])),
                10,
            ),
            (
                "barberos",
                documento(
                    "Reporte detallado de los barberos",
                    pdf::barberos(&(1..=6).map(|id| barbero(id, 300)).collect::<Vec<_>>()),
                ),
                30,
            ),
            (
                "clientes",
                documento("Reporte de clientes", pdf::clientes(&clientes())),
                10,
            ),
        ];
        for (nombre, documento, meta_segundos) in casos {
            let inicio = Instant::now();
            let bytes = gotenberg.generar(&documento).await.unwrap();
            let segundos = inicio.elapsed().as_secs_f64();
            eprintln!(
                "{nombre}.pdf: {} bytes en {segundos:.2} s (meta RNF-REN-04: {meta_segundos} s)",
                bytes.len()
            );
            assert!(bytes.starts_with(b"%PDF") && bytes.windows(5).any(|v| v == b"%%EOF"));
            assert!(segundos < f64::from(meta_segundos), "{nombre}: {segundos:.2} s");
            if let Ok(dir) = std::env::var("PRUEBAS_PDF_DIR") {
                std::fs::write(format!("{dir}/{nombre}.pdf"), &bytes).unwrap();
            }
        }
    }

    #[tokio::test]
    async fn sin_gotenberg_el_error_es_claro() {
        let gotenberg = Gotenberg::nuevo("http://127.0.0.1:1/").unwrap();
        let documento = DocumentoPdf {
            cuerpo: "<p>x</p>".into(),
            encabezado: String::new(),
            pie: String::new(),
        };
        let error = gotenberg.generar(&documento).await.unwrap_err();
        assert!(error.0.contains("Gotenberg no responde"), "{error:?}");
    }
}
