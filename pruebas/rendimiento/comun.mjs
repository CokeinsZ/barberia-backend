// Utilidades de las pruebas de rendimiento: peticiones con la sesión y el Origin del sitio.
export const API = process.env.API_URL ?? 'http://localhost:8080';
export const ORIGEN = process.env.ORIGEN ?? API;

export async function pedir(metodo, ruta, { sesion, cuerpo } = {}) {
  const cabeceras = { Origin: ORIGEN };
  if (sesion) cabeceras.Cookie = `__Host-sesion=${sesion}`;
  if (cuerpo !== undefined) cabeceras['Content-Type'] = 'application/json';
  const inicio = performance.now();
  const respuesta = await fetch(`${API}${ruta}`, {
    method: metodo,
    headers: cabeceras,
    body: cuerpo === undefined ? undefined : JSON.stringify(cuerpo),
  });
  const bytes = Buffer.from(await respuesta.arrayBuffer());
  const ms = performance.now() - inicio;
  const tipo = respuesta.headers.get('content-type') ?? '';
  const json = tipo.includes('json') && bytes.length ? JSON.parse(bytes.toString('utf8')) : null;
  return { estado: respuesta.status, cabeceras: respuesta.headers, json, bytes, ms };
}

export function exigir(r, esperado, contexto) {
  if (r.estado !== esperado) {
    throw new Error(`${contexto}: se esperaba ${esperado} y llegó ${r.estado} ${r.json ? JSON.stringify(r.json) : ''}`);
  }
  return r;
}

export function hoyBogota(desplazamientoDias = 0) {
  const ahora = new Date(Date.now() + desplazamientoDias * 86400000);
  return new Intl.DateTimeFormat('en-CA', { timeZone: 'America/Bogota' }).format(ahora);
}
