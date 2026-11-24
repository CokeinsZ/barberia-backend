#!/usr/bin/env node
// Prueba E2E de WhatsApp de verdad (Meta) contra el servidor desplegado: manda cada plantilla
// a UN solo celular, el suyo, y muestra cómo queda cada mensaje (enviado, entregado, leído o
// fallido con el motivo de Meta).
//
//   DIAMANTE_URL=https://back-diamante.mania-agent.online CELULAR=573003579384 node whatsapp-real.mjs
//   (o: make whatsapp-real URL=… CELULAR=…)
//
// Con --interactivo (o en una terminal, si no pasa --sin-preguntas) además:
//   - le pide tocar «Confirmo» en el recordatorio y espera a que el webhook confirme la cita;
//   - le pide el código que le llegó y hace una reserva en línea con él (requiere una política
//     de tratamiento de datos vigente; si no la hay, lo dice y solo envía el código).
//
// Crea un barbero y un servicio propios (ocultos en la landing) y SIEMPRE los deja retirado y
// desactivado, aunque la prueba falle. Al empezar también recoge los que haya dejado una corrida
// anterior que se cortó. Con --solo-limpiar hace solo eso (no envía nada; no pide CELULAR).
// El cliente es el de su celular: si ya existía, se usa ese.

import { createInterface } from 'node:readline/promises';
import { randomBytes } from 'node:crypto';
import { chmodSync, existsSync, readFileSync, writeFileSync } from 'node:fs';
import { Cliente, cookieDeSesion } from './lib/cliente.mjs';
import * as d from './lib/datos.mjs';

const aqui = ruta => new URL(ruta, import.meta.url);
const base = (process.env.DIAMANTE_URL ?? '').replace(/\/+$/, '');
const crudo = (process.env.CELULAR ?? '').replace(/\D/g, '');
const celular = crudo.length === 12 && crudo.startsWith('57') ? crudo.slice(2) : crudo;
const argumentos = new Set(process.argv.slice(2));
const soloLimpiar = argumentos.has('--solo-limpiar');
const interactivo =
  !soloLimpiar && (argumentos.has('--interactivo') || (process.stdin.isTTY && !argumentos.has('--sin-preguntas')));

// Nombres de lo que crea la prueba: así se reconocen los restos de una corrida cortada.
const PREFIJO_SERVICIO = 'Prueba WhatsApp WA ';
const PREFIJO_APELLIDOS = 'Prueba WA ';

/** Un paso que no salió como se esperaba: corta la prueba, pero la limpieza igual se hace. */
class Fallo extends Error {}

function salir(mensaje) {
  console.error(`✘ ${mensaje}`);
  process.exit(2);
}
if (!base.startsWith('https://')) salir('Falta DIAMANTE_URL=https://… (el servidor desplegado)');
if (!soloLimpiar && !/^3\d{9}$/.test(celular)) salir('Falta CELULAR: su celular colombiano, 3XXXXXXXXX o 573XXXXXXXXX');
const anonimo = new Cliente(base);

// Credenciales de la administradora: las de la batería (.credenciales.json). Con
// DIAMANTE_TOKEN_ACTIVACION (el token de `diamante crear-administrador`) se activa la cuenta con
// una contraseña aleatoria y se guardan de nuevo (p. ej., tras reiniciar la base).
const rutaCredenciales = aqui('./.credenciales.json');
let credenciales = existsSync(rutaCredenciales) ? JSON.parse(readFileSync(rutaCredenciales, 'utf8')) : null;
if (process.env.DIAMANTE_TOKEN_ACTIVACION) {
  const correo = process.env.DIAMANTE_CORREO_ADMIN ?? credenciales?.correo;
  if (!correo) salir('Con DIAMANTE_TOKEN_ACTIVACION defina también DIAMANTE_CORREO_ADMIN');
  const contrasena = randomBytes(24).toString('base64url');
  const r = await anonimo.post('/api/v1/auth/activacion', { token: process.env.DIAMANTE_TOKEN_ACTIVACION, contrasena });
  if (r.estado !== 204) salir(`no se pudo activar la cuenta: ${r.estado} ${JSON.stringify(r.json)}`);
  credenciales = { correo, contrasena };
  writeFileSync(rutaCredenciales, JSON.stringify(credenciales, null, 2));
  chmodSync(rutaCredenciales, 0o600);
  console.log(`✔ Cuenta de ${correo} activada; credenciales en .credenciales.json`);
}
if (!credenciales) salir('Faltan las credenciales de la administradora: use DIAMANTE_TOKEN_ACTIVACION y DIAMANTE_CORREO_ADMIN');

const consola = interactivo ? createInterface({ input: process.stdin, output: process.stdout }) : null;
const esperarMs = ms => new Promise(listo => setTimeout(listo, ms));
const resultados = [];
const enviados = new Map();
let admin = null;

function exigir(r, estados, que) {
  const lista = Array.isArray(estados) ? estados : [estados];
  if (!lista.includes(r.estado)) throw new Fallo(`${que}: ${r.metodo} ${r.url} → ${r.estado} ${JSON.stringify(r.json)}`);
  return r.json;
}

/**
 * Retira a los barberos y desactiva los servicios de esta prueba que sigan activos (los de esta
 * corrida y los de una anterior que se cortó), cancelando antes sus citas pendientes.
 */
async function limpiar() {
  const hoy = d.hoy();
  const resumen = [];
  const { datos: barberos } = exigir(await admin.get('/api/v1/barberos'), 200, 'listar barberos');
  for (const b of barberos.filter(b => b.apellidos.startsWith(PREFIJO_APELLIDOS) && !b.fecha_retiro)) {
    const citas = exigir(
      await admin.get(`/api/v1/agenda/citas?barbero_id=${b.id}&desde=${hoy}&hasta=${d.fecha(45)}`),
      200,
      'citas del barbero de prueba',
    );
    for (const c of citas.filter(c => c.estado === 'sin_confirmar' || c.estado === 'confirmada')) {
      exigir(
        await admin.post(`/api/v1/agenda/citas/${c.id}/cancelar`, { motivo: 'otro', avisar_cliente: false }),
        200,
        'cancelar una cita de prueba',
      );
    }
    exigir(await admin.post(`/api/v1/barberos/${b.id}/retiro`, { fecha: hoy, motivo: 'otro' }), 200, 'retirar el barbero de prueba');
    resumen.push(`barbero ${b.id}`);
  }
  const servicios = exigir(await admin.get('/api/v1/servicios'), 200, 'listar servicios');
  for (const s of servicios.filter(s => s.activo && s.nombre.startsWith(PREFIJO_SERVICIO))) {
    exigir(await admin.patch(`/api/v1/servicios/${s.id}/estado`, { activo: false }), [200, 204], 'desactivar el servicio de prueba');
    resumen.push(`servicio ${s.id}`);
  }
  return resumen;
}

/** Espera a que el último mensaje de esa plantilla salga de la cola y anota su estado. */
async function seguir(plantilla, desde) {
  for (let i = 0; i < 30; i++) {
    const r = exigir(await admin.get(`/api/v1/mensajes?destinatario=${celular}&plantilla=${plantilla}&por_pagina=5`), 200, 'registro de mensajes');
    const mensaje = (r.datos ?? r).find(m => Date.parse(m.creado_en) >= desde);
    if (mensaje && mensaje.estado !== 'pendiente') {
      enviados.set(mensaje.id, plantilla);
      const marca = mensaje.estado === 'fallido' || mensaje.estado === 'cancelado' ? '✘' : '✔';
      console.log(`  ${marca} ${plantilla}: ${mensaje.estado}${mensaje.ultimo_error ? ` — ${mensaje.ultimo_error}` : ''}`);
      return mensaje;
    }
    await esperarMs(2000);
  }
  console.log(`  ✘ ${plantilla}: sigue en la cola después de 60 s (¿el worker está corriendo?)`);
  return null;
}

async function paso(titulo, plantilla, accion) {
  const desde = Date.now() - 2000;
  console.log(`▶ ${titulo}`);
  await accion();
  const mensaje = await seguir(plantilla, desde);
  resultados.push({ plantilla, mensaje });
  await esperarMs(3000); // que lleguen en orden
  return mensaje;
}

async function prueba() {
  console.log(`WhatsApp de verdad · ${base}${soloLimpiar ? ' · solo limpieza' : ` · celular ${celular}`}${interactivo ? ' · interactivo' : ''}\n`);

  // 1. Sesión de la administradora y restos de una corrida anterior.
  const entrada = await anonimo.post('/api/v1/auth/sesion', { correo: credenciales.correo, contrasena: credenciales.contrasena }, { reintentar429: false });
  exigir(entrada, 200, 'inicio de sesión');
  admin = anonimo.conSesion(cookieDeSesion(entrada), 'administradora');
  const restos = await limpiar();
  if (restos.length) console.log(`✔ Restos de una corrida anterior limpiados: ${restos.join(', ')}`);
  if (soloLimpiar) {
    if (!restos.length) console.log('✔ No había nada que limpiar');
    return 0;
  }

  // 2. El servidor envía por Meta.
  const estado = exigir(await admin.get('/api/v1/sistema/estado'), 200, 'estado del sistema');
  if (estado.cola?.proveedor_whatsapp !== 'meta') {
    throw new Fallo(`el servidor no envía por Meta (proveedor: ${estado.cola?.proveedor_whatsapp ?? 'ninguno'}). Revise DIAMANTE_WHATSAPP_TOKEN y DIAMANTE_WHATSAPP_ID_NUMERO y despliegue la versión con el adaptador.`);
  }
  console.log('✔ El servidor envía WhatsApp por Meta');
  if (estado.cola.whatsapp_solo_a) {
    console.log(`  ⚠ Solo a ${estado.cola.whatsapp_solo_a} celular(es) de DIAMANTE_WHATSAPP_SOLO_A: el suyo debe estar en la lista, o los mensajes se simulan y no le llegan.`);
  }

  // 3. Barbero y servicio propios, que trabajan todo el día (siempre hay horas libres).
  const sufijo = `WA ${d.corrida}`;
  const servicio = exigir(await admin.post('/api/v1/servicios', {
    categoria_id: 1, nombre: `${PREFIJO_SERVICIO}${d.corrida}`, descripcion: 'Servicio de la prueba de WhatsApp.',
    precio: 1000, duracion_min: 30, agendable_en_linea: true, visible_en_landing: false,
  }), 201, 'crear el servicio').id;
  const barbero = exigir(await admin.post('/api/v1/barberos', {
    tipo_documento: 'CC', numero_documento: d.documento(), nombres: 'Barbero', apellidos: `Prueba ${sufijo}`,
    fecha_nacimiento: '1995-03-02', celular: '310 555 9917', correo: d.correo('barbero-whatsapp'), especialidad: 'Pruebas',
    anios_experiencia: 1, fecha_ingreso: '2025-01-15', tipo_vinculacion: 'prestacion_servicios',
    servicios: [servicio], horario: d.horarioCompleto,
    visibilidad: { mostrar_en_landing: false, recibe_citas_en_linea: true, acceso_panel: false },
  }), 201, 'crear el barbero').id;
  console.log(`✔ Barbero ${barbero} y servicio ${servicio} de la prueba`);

  // 4. El cliente es el de su celular.
  let clienteId;
  const nuevo = await admin.post('/api/v1/clientes', { nombre: process.env.NOMBRE ?? 'Prueba WhatsApp', celular });
  if (nuevo.estado === 201) {
    clienteId = nuevo.json.id;
  } else if (nuevo.json?.codigo === 'celular_en_uso') {
    const encontrados = exigir(await admin.get(`/api/v1/clientes/buscar?q=${celular}`), 200, 'buscar el cliente');
    clienteId = encontrados.find(c => c.celular?.replace(/\D/g, '') === celular)?.id;
  }
  if (!clienteId) throw new Fallo(`no se pudo registrar ni encontrar al cliente del celular: ${JSON.stringify(nuevo.json)}`);
  console.log(`✔ Cliente ${clienteId} (su celular)\n`);

  // 5. Una cita por el panel: confirmación, recordatorio, cambio, aviso y cancelación.
  const manana = d.fecha(1);
  let cita;
  await paso('Cita agendada por el panel', 'cita_confirmada', async () => {
    cita = exigir(await admin.post('/api/v1/agenda/citas', { cliente_id: clienteId, servicio_id: servicio, barbero_id: barbero, inicio: d.instante(manana, '10:00') }), 201, 'crear la cita').id;
  });
  await paso('Recordatorio con los botones «Confirmo» y «Cancelar cita»', 'recordatorio_cita', async () => {
    exigir(await admin.post(`/api/v1/agenda/citas/${cita}/recordatorio`), 200, 'recordatorio');
  });
  if (interactivo) {
    await consola.question('\n  Toque «Confirmo» en el recordatorio que le llegó y oprima Enter (o Enter para saltar)… ');
    let confirmada = false;
    for (let i = 0; i < 45 && !confirmada; i++) {
      confirmada = exigir(await admin.get(`/api/v1/agenda/citas/${cita}`), 200, 'detalle de la cita').estado === 'confirmada';
      if (!confirmada) await esperarMs(2000);
    }
    console.log(confirmada
      ? '  ✔ El webhook de Meta llegó: la cita quedó «Confirmada»\n'
      : '  ✘ La cita no se confirmó en 90 s: revise el webhook en Meta (URL, token de verificación y el campo «messages»)\n');
    resultados.push({ plantilla: 'botón «Confirmo» (webhook)', mensaje: { estado: confirmada ? 'confirmada' : 'sin respuesta' } });
  }
  await paso('Cambio de hora (reprogramación por el panel)', 'cita_reprogramada', async () => {
    exigir(await admin.post(`/api/v1/agenda/citas/${cita}/reprogramar`, { inicio: d.instante(manana, '11:00'), barbero_id: barbero }), 200, 'reprogramar');
  });
  await paso('Aviso para elegir otra hora', 'aviso_reprogramacion', async () => {
    exigir(await admin.post('/api/v1/agenda/citas/aviso-reprogramacion', { cita_ids: [cita] }), 200, 'aviso de reprogramación');
  });
  await paso('Cancelación avisando al cliente', 'cita_cancelada', async () => {
    exigir(await admin.post(`/api/v1/agenda/citas/${cita}/cancelar`, { motivo: 'otro', avisar_cliente: true }), 200, 'cancelar');
  });

  // 6. El código de verificación y, si hay alguien al otro lado, una reserva en línea con él.
  //    Sin una política de tratamiento de datos vigente nadie puede reservar en línea: se avisa
  //    ANTES de pedir el código, para no gastarlo.
  const politicas = exigir(await admin.get('/api/v1/negocio/politicas'), 200, 'políticas');
  const politica = politicas.find(p => p.tipo === 'tratamiento_datos' && p.vigente)?.id;
  let reservaOmitida = null;
  if (interactivo && !politica) {
    reservaOmitida = 'no hay una política de tratamiento de datos vigente (sin ella nadie puede reservar en línea). Publíquela en el panel o con POST /api/v1/negocio/politicas';
    console.log(`\n  ⚠ La reserva en línea no se probará: ${reservaOmitida}. Solo se envía el código.\n`);
  }
  let verificacion;
  await paso('Código de verificación', 'codigo_verificacion', async () => {
    const r = await anonimo.post('/api/v1/publico/verificaciones', { celular }, { reintentar429: false });
    if (r.estado === 429) {
      console.log(`  (límite de códigos: reintente en ${r.cabeceras.get('retry-after')} s)`);
      return;
    }
    verificacion = exigir(r, 202, 'pedir el código').id;
  });
  if (interactivo && verificacion && politica) {
    const codigo = (await consola.question('\n  Escriba el código de 4 dígitos que le llegó (o Enter para saltar): ')).trim();
    if (/^\d{4}$/.test(codigo)) {
      const confirmacion = await anonimo.post(`/api/v1/publico/verificaciones/${verificacion}/confirmar`, { codigo });
      if (confirmacion.estado === 200) {
        const nombre = confirmacion.json.cliente?.nombre ?? process.env.NOMBRE ?? 'Prueba WhatsApp';
        let enLinea;
        await paso('Reserva en línea con el código', 'cita_confirmada', async () => {
          const creada = exigir(await anonimo.post('/api/v1/publico/citas', {
            comprobante: confirmacion.json.comprobante, servicio_id: servicio, barbero_id: barbero, inicio: d.instante(manana, '12:00'),
            cliente: { nombre, correo: null, fecha_nacimiento: null }, autoriza_tratamiento_datos: true, politica_version_id: politica,
          }), 201, 'reservar en línea');
          console.log(`  Enlace de gestión: ${creada.enlace_gestion}`);
          enLinea = creada.cita.id; // `201 { cita, enlace_gestion, calendario_google }`
        });
        exigir(await admin.post(`/api/v1/agenda/citas/${enLinea}/cancelar`, { motivo: 'otro', avisar_cliente: false }), 200, 'cancelar la reserva de prueba');
      } else {
        console.log(`  ✘ El código no sirvió: ${confirmacion.estado} ${JSON.stringify(confirmacion.json)}`);
        resultados.push({ plantilla: 'confirmar el código', mensaje: null });
      }
    }
  }

  // 7. Entregas: Meta avisa por el webhook (entregado, leído). Se espera un poco y se muestra todo.
  console.log('\nEsperando las confirmaciones de entrega de Meta (30 s)…');
  await esperarMs(30000);
  const r = exigir(await admin.get(`/api/v1/mensajes?destinatario=${celular}&por_pagina=50`), 200, 'registro de mensajes');
  console.log('\nEstado final de cada mensaje (enviado → entregado → leído llegan por el webhook):');
  for (const m of (r.datos ?? r).filter(m => enviados.has(m.id))) {
    console.log(`  ${m.estado.padEnd(10)} ${m.plantilla}${m.ultimo_error ? ` — ${m.ultimo_error}` : ''}`);
  }
  const fallidos = (r.datos ?? r).filter(m => enviados.has(m.id) && ['fallido', 'cancelado'].includes(m.estado));
  const sinEnviar = resultados.filter(x => !x.mensaje).length;
  if (reservaOmitida) console.log(`\n⚠ Reserva en línea sin probar: ${reservaOmitida}.`);
  console.log(fallidos.length || sinEnviar ? `\n✘ ${fallidos.length} fallidos, ${sinEnviar} sin salir de la cola` : '\n✔ Todos los mensajes salieron por Meta');
  return fallidos.length || sinEnviar ? 1 : 0;
}

let codigoSalida;
try {
  codigoSalida = await prueba();
} catch (error) {
  console.error(`\n✘ ${error instanceof Fallo ? error.message : error.stack}`);
  codigoSalida = 2;
} finally {
  consola?.close();
  if (admin) {
    try {
      const limpiados = await limpiar();
      if (limpiados.length && !soloLimpiar) console.log(`\n✔ Limpieza: ${limpiados.join(' y ')} (retirado y desactivado)`);
    } catch (error) {
      console.error(`\n✘ La limpieza falló (corra con --solo-limpiar): ${error.message}`);
      codigoSalida ||= 2;
    }
    await admin.delete('/api/v1/auth/sesion').catch(() => {});
  }
}
process.exit(codigoSalida);
