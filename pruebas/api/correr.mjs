// Corre la colección de Postman (docs/pruebas-manuales) con newman y guarda las variables de
// colección al terminar (newman no las exporta), para seguir en otra corrida:
//
//   CARPETAS=00,01,…,20 node correr.mjs salida.json [entrada.json] [clave=valor…]
//
// Ver README.md: entre la carpeta 20 y la 21 hay que reiniciar la API (límite de inicios de
// sesión por IP).
import { readFileSync, writeFileSync, existsSync, mkdtempSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import newman from 'newman';

const RUTA = fileURLToPath(new URL('../../docs/pruebas-manuales/diamante-barberia.postman_collection.json', import.meta.url));
const [salida, entrada, ...pares] = process.argv.slice(2);
if (!salida) {
  console.error('Uso: CARPETAS=00,01 node correr.mjs salida.json [entrada.json] [clave=valor…]');
  process.exit(2);
}
const coleccion = JSON.parse(readFileSync(RUTA, 'utf8'));
const valores = entrada && existsSync(entrada) ? JSON.parse(readFileSync(entrada, 'utf8')) : {};
for (const par of pares) {
  const i = par.indexOf('=');
  valores[par.slice(0, i)] = par.slice(i + 1);
}
for (const [clave, valor] of Object.entries(valores)) {
  const v = coleccion.variable.find(x => x.key === clave);
  if (v) v.value = String(valor);
  else coleccion.variable.push({ key: clave, value: String(valor) });
}
// Las cargas de archivos de la colección piden elegir el archivo a mano: aquí se usan una foto
// PNG vertical (solo la cabecera, basta para detectar tipo y medidas) y un PDF mínimo.
const archivos = mkdtempSync(join(tmpdir(), 'diamante-coleccion-'));
const png = Buffer.alloc(33);
Buffer.from('\x89PNG\r\n\x1a\n\0\0\0\x0dIHDR', 'latin1').copy(png, 0);
png.writeUInt32BE(300, 16);
png.writeUInt32BE(400, 20);
png.set([8, 2, 0, 0, 0], 24);
writeFileSync(join(archivos, 'foto.png'), png);
writeFileSync(join(archivos, 'documento.pdf'), '%PDF-1.4\n1 0 obj << /Type /Catalog >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF\n');
const rellenar = items => {
  for (const item of items) {
    if (item.item) rellenar(item.item);
    for (const campo of item.request?.body?.formdata ?? []) {
      if (campo.type === 'file' && (!campo.src || campo.src.length === 0)) {
        campo.src = join(archivos, campo.key === 'foto' ? 'foto.png' : 'documento.pdf');
      }
    }
  }
};
rellenar(coleccion.item);

const prefijos = (process.env.CARPETAS ?? '').split(',').filter(Boolean);
const carpetas = prefijos.length
  ? coleccion.item.filter(f => prefijos.some(p => f.name.startsWith(p))).map(f => f.name)
  : undefined;

// Variables de colección vivas: las trae el resultado de cada script.
let vivas = null;
newman.run({ collection: coleccion, folder: carpetas, reporters: ['cli'], timeoutRequest: 180000 }, (error, resumen) => {
  if (error) {
    console.error(error);
    process.exit(2);
  }
  const variables = { ...valores };
  vivas?.values?.each(v => { variables[v.key] = v.value; });
  writeFileSync(salida, JSON.stringify(variables, null, 2));
  const s = resumen.run.stats;
  console.log(`\nPeticiones: ${s.requests.total} (fallidas ${s.requests.failed}) · Pruebas: ${s.assertions.total} (fallidas ${s.assertions.failed})`);
  for (const f of resumen.run.failures) console.log(`  ✗ ${f.source?.name} :: ${f.error?.message}`);
  process.exit(s.assertions.failed || s.requests.failed ? 1 : 0);
}).on('script', (_error, datos) => {
  if (datos?.execution?.collectionVariables) vivas = datos.execution.collectionVariables;
});
