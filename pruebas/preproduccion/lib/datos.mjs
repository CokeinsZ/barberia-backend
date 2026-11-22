// Datos únicos por corrida (la base de preproducción es persistente) y fechas de Bogotá.
import { deflateSync } from 'node:zlib';

const marca = new Date();
const segundo = Math.floor(marca.getTime() / 1000);

/// Identificador de la corrida (AAMMDDHHMMSS, hora de Bogotá): único aunque se repita enseguida.
export const corrida = new Intl.DateTimeFormat('sv-SE', {
  timeZone: 'America/Bogota', year: '2-digit', month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit',
}).format(marca).replace(/\D/g, '');

/// Sufijo de los nombres de lo que crea la corrida, p. ej. «E2E 2611212140».
export const sufijo = `E2E ${corrida}`;

let contadorCelular = 0;
/// Celular colombiano único por corrida: 3 + 7 cifras del segundo + 2 de contador.
export function celular() {
  contadorCelular++;
  return `3${String(segundo % 10_000_000).padStart(7, '0')}${String(contadorCelular).padStart(2, '0')}`;
}

let contadorDocumento = 0;
export function documento() {
  contadorDocumento++;
  // Cédula de 10 dígitos: 9 + 7 cifras del segundo + 2 de contador.
  return `9${String(segundo % 10_000_000).padStart(7, '0')}${String(contadorDocumento).padStart(2, '0')}`;
}

export function correo(nombre) {
  // example.com no recibe correo (RFC 2606): las invitaciones no le llegan a nadie.
  return `${nombre}.${corrida}@example.com`;
}

/// Fecha de Bogotá (AAAA-MM-DD) desplazada `dias` desde hoy.
export function fecha(dias = 0) {
  return new Intl.DateTimeFormat('en-CA', { timeZone: 'America/Bogota' }).format(new Date(Date.now() + dias * 86_400_000));
}

export function instante(fechaTexto, hora) {
  return `${fechaTexto}T${hora}:00-05:00`;
}

export const hoy = () => fecha(0);

/// Horario de 00:00 a 23:30 los siete días: así siempre hay turnos «de hoy» para las pruebas.
export const horarioCompleto = [1, 2, 3, 4, 5, 6, 7].map(dia => ({ dia_semana: dia, inicio: '00:00', fin: '23:30' }));

/// Una foto PNG válida (vertical, 300×400, un color) para la foto del barbero.
export function fotoPng(ancho = 300, alto = 400) {
  const crc = (() => {
    const tabla = new Uint32Array(256).map((_, n) => {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      return c >>> 0;
    });
    return b => { let c = 0xffffffff; for (const x of b) c = tabla[(c ^ x) & 0xff] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  })();
  const bloque = (tipo, datos) => {
    const t = Buffer.from(tipo, 'ascii');
    const largo = Buffer.alloc(4); largo.writeUInt32BE(datos.length);
    const suma = Buffer.alloc(4); suma.writeUInt32BE(crc(Buffer.concat([t, datos])));
    return Buffer.concat([largo, t, datos, suma]);
  };
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(ancho, 0); ihdr.writeUInt32BE(alto, 4); ihdr.set([8, 2, 0, 0, 0], 8);
  const fila = Buffer.concat([Buffer.from([0]), Buffer.alloc(ancho * 3, 0)]);
  for (let x = 0; x < ancho; x++) fila.set([201, 163, 90], 1 + x * 3);
  const crudo = Buffer.concat(Array.from({ length: alto }, () => fila));
  return Buffer.concat([
    Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
    bloque('IHDR', ihdr), bloque('IDAT', deflateSync(crudo)), bloque('IEND', Buffer.alloc(0)),
  ]);
}

/// Un PDF mínimo válido.
export function pdf(texto = 'Documento de prueba') {
  const contenido = `BT /F1 18 Tf 72 720 Td (${texto}) Tj ET`;
  const objetos = [
    '<< /Type /Catalog /Pages 2 0 R >>',
    '<< /Type /Pages /Kids [3 0 R] /Count 1 >>',
    '<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>',
    `<< /Length ${contenido.length} >>\nstream\n${contenido}\nendstream`,
    '<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>',
  ];
  let salida = '%PDF-1.4\n';
  const posiciones = [];
  objetos.forEach((o, i) => { posiciones.push(salida.length); salida += `${i + 1} 0 obj\n${o}\nendobj\n`; });
  const xref = salida.length;
  salida += `xref\n0 ${objetos.length + 1}\n0000000000 65535 f \n${posiciones.map(p => `${String(p).padStart(10, '0')} 00000 n \n`).join('')}`;
  salida += `trailer\n<< /Size ${objetos.length + 1} /Root 1 0 R >>\nstartxref\n${xref}\n%%EOF\n`;
  return Buffer.from(salida, 'latin1');
}
