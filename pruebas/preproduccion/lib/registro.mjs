// Registro de resultados: cada paso pasa, falla u se omite (con su motivo), y un paso que falla
// no detiene a los demás.
import assert from 'node:assert/strict';
import { describir } from './cliente.mjs';

export class Omitido extends Error {}

export function omitir(motivo) {
  throw new Omitido(motivo);
}

export class Registro {
  constructor() {
    this.resultados = [];
    this.escenario = '';
  }

  inicio(escenario) {
    this.escenario = escenario;
    console.log(`\n▶ ${escenario}`);
  }

  /// Corre un paso. `requiere`: valores que deben existir (de pasos anteriores) o se omite.
  async paso(nombre, fn, { requiere = [] } = {}) {
    const inicio = performance.now();
    const fila = { escenario: this.escenario, paso: nombre };
    try {
      if (requiere.some(v => v === undefined || v === null || v === false)) omitir('depende de un paso anterior que no se completó');
      const valor = await fn();
      fila.resultado = 'pasó';
      this.resultados.push({ ...fila, ms: Math.round(performance.now() - inicio) });
      console.log(`  ✔ ${nombre}`);
      return valor;
    } catch (error) {
      fila.ms = Math.round(performance.now() - inicio);
      if (error instanceof Omitido) {
        fila.resultado = 'omitido';
        fila.motivo = error.message;
        console.log(`  ○ ${nombre} — ${error.message}`);
      } else {
        fila.resultado = 'falló';
        fila.motivo = error.message;
        console.log(`  ✘ ${nombre}\n      ${error.message.split('\n').join('\n      ')}`);
      }
      this.resultados.push(fila);
      return undefined;
    }
  }

  resumen() {
    const cuenta = r => this.resultados.filter(x => x.resultado === r).length;
    return { pasaron: cuenta('pasó'), fallaron: cuenta('falló'), omitidos: cuenta('omitido'), total: this.resultados.length };
  }
}

/// Exige un estado HTTP (o uno de varios) y devuelve el JSON.
export function esperar(r, estados, contexto = '') {
  const lista = Array.isArray(estados) ? estados : [estados];
  if (!lista.includes(r.estado)) {
    throw new Error(`${contexto ? contexto + ': ' : ''}se esperaba ${lista.join(' o ')} y llegó ${describir(r)}`);
  }
  return r.json;
}

/// Exige un error con su código (`codigo` del Problem Details).
export function esperarError(r, estado, codigo, contexto = '') {
  esperar(r, estado, contexto);
  if (codigo) assert.equal(r.json?.codigo, codigo, `${contexto}: código ${r.json?.codigo} en lugar de ${codigo}`);
  return r.json;
}

export { assert };
