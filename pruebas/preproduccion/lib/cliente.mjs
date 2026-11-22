// Cliente HTTP de las pruebas: sesión por cookie, Origin del sitio, JSON o multipart, y
// reintento automático ante 429 (respeta Retry-After) para que los límites de uso de
// producción no conviertan en fallas las pruebas que no los están probando.

export class Cliente {
  constructor(base, { origen, nombre = 'anónimo' } = {}) {
    this.base = base.replace(/\/$/, '');
    this.origen = origen ?? this.base;
    this.nombre = nombre;
    this.sesion = null;
    this.esperas429 = 0;
  }

  conSesion(sesion, nombre) {
    const otro = new Cliente(this.base, { origen: this.origen, nombre });
    otro.sesion = sesion;
    return otro;
  }

  async pedir(metodo, ruta, opciones = {}) {
    const { cuerpo, formulario, origen = this.origen, reintentar429 = true, cabeceras = {}, sinSesion = false } = opciones;
    const url = ruta.startsWith('http') ? ruta : `${this.base}${ruta}`;
    for (let intento = 0; ; intento++) {
      const h = { Origin: origen, ...cabeceras };
      if (this.sesion && !sinSesion) h.Cookie = `__Host-sesion=${this.sesion}`;
      let body;
      if (formulario) {
        body = formulario();
      } else if (cuerpo !== undefined) {
        h['Content-Type'] = 'application/json';
        body = typeof cuerpo === 'string' ? cuerpo : JSON.stringify(cuerpo);
      }
      const inicio = performance.now();
      const respuesta = await fetch(url, { method: metodo, headers: h, body, signal: AbortSignal.timeout(150_000) });
      const bytes = Buffer.from(await respuesta.arrayBuffer());
      const ms = performance.now() - inicio;
      if (respuesta.status === 429 && reintentar429 && intento < 4) {
        const espera = Math.min(70, Number(respuesta.headers.get('retry-after') ?? 5));
        this.esperas429++;
        await new Promise(listo => setTimeout(listo, (espera + 1) * 1000));
        continue;
      }
      const tipo = respuesta.headers.get('content-type') ?? '';
      let json = null;
      if (tipo.includes('json') && bytes.length) {
        try { json = JSON.parse(bytes.toString('utf8')); } catch { json = null; }
      }
      return { estado: respuesta.status, json, bytes, cabeceras: respuesta.headers, ms, url, metodo };
    }
  }

  get(ruta, o) { return this.pedir('GET', ruta, o); }
  post(ruta, cuerpo, o = {}) { return this.pedir('POST', ruta, { ...o, cuerpo }); }
  put(ruta, cuerpo, o = {}) { return this.pedir('PUT', ruta, { ...o, cuerpo }); }
  patch(ruta, cuerpo, o = {}) { return this.pedir('PATCH', ruta, { ...o, cuerpo }); }
  delete(ruta, o) { return this.pedir('DELETE', ruta, o); }

  /// multipart/form-data: campos de texto y archivos `{ campo: { nombre, tipo, contenido } }`.
  subir(ruta, campos, o = {}) {
    return this.pedir('POST', ruta, {
      ...o,
      formulario: () => {
        const f = new FormData();
        for (const [clave, valor] of Object.entries(campos)) {
          if (typeof valor === 'string') f.append(clave, valor);
          else f.append(clave, new Blob([valor.contenido], { type: valor.tipo }), valor.nombre);
        }
        return f;
      },
    });
  }
}

/// La cookie de sesión de una respuesta de inicio de sesión.
export function cookieDeSesion(r) {
  const valor = r.cabeceras.get('set-cookie') ?? '';
  return valor.match(/__Host-sesion=([^;]*)/)?.[1] ?? null;
}

/// Descripción corta de una respuesta para los mensajes de error.
export function describir(r) {
  const cuerpo = r.json ? JSON.stringify(r.json) : r.bytes.subarray(0, 200).toString('utf8');
  return `${r.metodo} ${r.url.replace(/^https?:\/\/[^/]+/, '')} → ${r.estado} ${cuerpo.slice(0, 400)}`;
}
