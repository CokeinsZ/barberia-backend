// Prueba de carga (RNF-REN-02 a 04). Cada escenario corre `DURACION` segundos con varios
// clientes a la vez y reporta percentiles de la latencia vista por el cliente (incluye la red
// local, así que es una cota superior del tiempo en el servidor).
//
//   node carga.mjs            (DURACION=20 por defecto)
import { readFileSync, writeFileSync } from 'node:fs';
import { pedir, hoyBogota } from './comun.mjs';

const { sesion, servicios, barberos } = JSON.parse(readFileSync(new URL('./base.json', import.meta.url)));
const DURACION = Number(process.env.DURACION ?? 20) * 1000;
const hoy = hoyBogota();
const ayer = hoyBogota(-1);
const mes = hoy.slice(0, 7);
const mesSiguiente = hoyBogota(31).slice(0, 7);
const [corte] = servicios;
const dias = [1, 2, 3, 4, 5, 6, 7].map(d => hoyBogota(d));
const mesAnterior = (() => { const [a, m] = mes.split('-').map(Number); return m === 1 ? `${a - 1}-12` : `${a}-${String(m - 1).padStart(2, '0')}`; })();
let contador = 0;
const sig = lista => lista[contador++ % lista.length];

const escenarios = [
  // RNF-REN-02: disponibilidad de un mes ≤ 500 ms (p95).
  { nombre: 'Disponibilidad del mes, cualquier barbero', meta: 500, req: 'RNF-REN-02', concurrencia: 10,
    peticion: () => pedir('GET', `/api/v1/publico/disponibilidad/calendario?servicio_id=${corte}&mes=${sig([mes, mesSiguiente])}`) },
  { nombre: 'Disponibilidad del mes, un barbero', meta: 500, req: 'RNF-REN-02', concurrencia: 10,
    peticion: () => pedir('GET', `/api/v1/publico/disponibilidad/calendario?servicio_id=${corte}&barbero_id=${sig(barberos)}&mes=${sig([mes, mesSiguiente])}`) },
  { nombre: 'Horas libres de un día, cualquier barbero', meta: 500, req: 'RNF-REN-02', concurrencia: 10,
    peticion: () => pedir('GET', `/api/v1/publico/disponibilidad/horas?servicio_id=${corte}&fecha=${sig(dias)}`) },
  { nombre: 'Landing', meta: 300, req: 'RNF-REN-03', concurrencia: 10,
    peticion: () => pedir('GET', '/api/v1/publico/landing') },
  // RNF-REN-03: operaciones comunes del panel ≤ 300 ms (p95).
  { nombre: 'Agenda del día (listado)', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/agenda/citas?desde=${hoy}&hasta=${hoy}`, { sesion }) },
  { nombre: 'Tablero de la agenda', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/agenda/tablero?fecha=${sig(dias)}`, { sesion }) },
  { nombre: 'Clientes: listado con búsqueda', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/clientes?q=${encodeURIComponent(sig(['Cliente 12', 'cliente 4', '3200001']))}`, { sesion }) },
  { nombre: 'Clientes: búsqueda rápida', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/clientes/buscar?q=${encodeURIComponent(sig(['Cliente 33', '320000', 'liente 25']))}`, { sesion }) },
  { nombre: 'Caja del día', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', '/api/v1/ventas/caja', { sesion }) },
  { nombre: 'Ventas del día', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/ventas?fecha=${hoy}`, { sesion }) },
  { nombre: 'Dashboard de ayer (día normal)', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', `/api/v1/reportes/dashboard?fecha=${ayer}`, { sesion }) },
  { nombre: 'Dashboard de hoy', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('GET', '/api/v1/reportes/dashboard', { sesion }) },
  { nombre: 'Resumen del mes (vista previa)', meta: 1000, req: '—', concurrencia: 2,
    peticion: () => pedir('GET', `/api/v1/reportes/resumen?mes=${mesAnterior}`, { sesion }) },
  { nombre: 'Clientes del mes (vista previa)', meta: 1000, req: '—', concurrencia: 2,
    peticion: () => pedir('GET', `/api/v1/reportes/clientes?mes=${mesAnterior}`, { sesion }) },
  // Escribe ventas de hoy: va al final para no inflar los escenarios que leen el día.
  { nombre: 'Guardar una venta', meta: 300, req: 'RNF-REN-03', concurrencia: 4,
    peticion: () => pedir('POST', '/api/v1/ventas', { sesion, cuerpo: {
      cliente: { tipo: 'ocasional' }, barbero_id: sig(barberos), servicio_ids: [corte],
      pagos: [{ metodo_pago_id: 1, monto: 35000 }] } }) },
];

function percentil(ordenados, p) {
  return ordenados[Math.min(ordenados.length - 1, Math.ceil((p / 100) * ordenados.length) - 1)];
}

// FILTRO=texto corre solo los escenarios cuyo nombre lo contiene.
const filtro = (process.env.FILTRO ?? '').toLowerCase();
const resultados = [];
for (const e of escenarios.filter(x => x.nombre.toLowerCase().includes(filtro))) {
  const tiempos = [];
  const errores = {};
  const fin = Date.now() + DURACION;
  await Promise.all(Array.from({ length: e.concurrencia }, async () => {
    while (Date.now() < fin) {
      const r = await e.peticion();
      if (r.estado >= 400) errores[r.estado] = (errores[r.estado] ?? 0) + 1;
      else tiempos.push(r.ms);
    }
  }));
  tiempos.sort((a, b) => a - b);
  const fila = {
    escenario: e.nombre, requisito: e.req, concurrencia: e.concurrencia, peticiones: tiempos.length,
    por_segundo: Math.round(tiempos.length / (DURACION / 1000)),
    p50: Math.round(percentil(tiempos, 50)), p95: Math.round(percentil(tiempos, 95)),
    p99: Math.round(percentil(tiempos, 99)), max: Math.round(tiempos.at(-1)), meta_p95: e.meta,
    cumple: percentil(tiempos, 95) <= e.meta && Object.keys(errores).length === 0, errores,
  };
  resultados.push(fila);
  console.log(`${fila.cumple ? '✔' : '✘'} ${e.nombre}: p50 ${fila.p50} ms · p95 ${fila.p95} ms · p99 ${fila.p99} ms · ${fila.por_segundo}/s (meta p95 ${e.meta} ms)${Object.keys(errores).length ? ' errores ' + JSON.stringify(errores) : ''}`);
}

// RNF-REN-04: PDF de un mes ≤ 10 s y el de todos los barberos ≤ 30 s (uno a la vez).
for (const [nombre, ruta, meta] of filtro && !'pdf'.includes(filtro) ? [] : [
  ['PDF resumen del mes', `/api/v1/reportes/resumen.pdf?mes=${mesAnterior}`, 10000],
  ['PDF de un barbero (mes)', `/api/v1/reportes/barberos/${barberos[0]}.pdf?mes=${mesAnterior}`, 10000],
  ['PDF de clientes (mes)', `/api/v1/reportes/clientes.pdf?mes=${mesAnterior}`, 10000],
  ['PDF de todos los barberos (mes)', `/api/v1/reportes/barberos.pdf?mes=${mesAnterior}`, 30000],
]) {
  const r = await pedir('GET', ruta, { sesion });
  const fila = { escenario: nombre, requisito: 'RNF-REN-04', peticiones: 1, ms: Math.round(r.ms), bytes: r.bytes.length,
    meta: meta, cumple: r.estado === 200 && r.ms <= meta, estado: r.estado };
  resultados.push(fila);
  console.log(`${fila.cumple ? '✔' : '✘'} ${nombre}: ${fila.ms} ms, ${(r.bytes.length / 1024).toFixed(0)} KB (meta ${meta / 1000} s)${r.estado !== 200 ? ' estado ' + r.estado : ''}`);
}
writeFileSync(new URL('./resultados.json', import.meta.url), JSON.stringify(resultados, null, 2));
process.exit(resultados.every(r => r.cumple) ? 0 : 1);
