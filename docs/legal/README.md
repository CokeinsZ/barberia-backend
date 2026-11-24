# Documentos legales

| Documento | Para quién | Dónde se publica |
|---|---|---|
| [Política de tratamiento de datos](politica-tratamiento-datos.md) | Clientes | `/legal/tratamiento-de-datos` en el sitio |

Este archivo es para el equipo y para quien administre la barbería; **no se publica**.

## Estado

- **Versión 1.0 (desarrollo), publicada en preproducción el 23/11/2026** (`id` 1, URL relativa
  `/legal/tratamiento-de-datos`): con ella ya funciona la reserva en línea. Los datos del
  responsable dicen «por definir» y la fecha de nacimiento quedó como dato opcional del
  seguimiento del servicio.
- El frontend la muestra en `/legal/tratamiento-de-datos`
  (`frontend/src/components/legal/PoliticaDatosView.tsx`, con el mismo texto) y el pie de página
  la enlaza.

## Antes de pasar a producción

1. **Completar los datos del responsable.** Cambiar los «por definir» por la razón social, el
   NIT, la dirección, el correo y el WhatsApp, y nombrar al proveedor del servidor con su país.
   Publicar el texto nuevo como versión 1.1 en el panel y en la página del frontend.
2. **Confirmar qué pasa con la fecha de nacimiento.** El formulario de reserva la pide como
   opcional y el panel muestra «cumpleaños de la semana». La 1.0 la justifica como «conocerle
   mejor» dentro del seguimiento, lo que es débil. Hay dos opciones:
   - Quitarla del formulario. El backend ya la acepta vacía.
   - Agregar su finalidad a la política, por ejemplo un saludo de cumpleaños. Un saludo con
     descuento sería publicidad, y la política dice que no se envía publicidad.
3. **Usar la lista «por reactivar» solo para análisis.** Los reportes listan clientes que no han
   vuelto, con su celular. La política dice que solo se les escribe sobre sus citas. Usar esa
   lista para invitarlos a volver sería una finalidad nueva: habría que agregarla a la política y,
   mejor, pedir una autorización aparte (opcional).
4. **Pedir la autorización en el panel.** Hoy solo la reserva en línea guarda la prueba de la
   autorización (canal `en_linea`). Los clientes registrados en la barbería (alta de cliente, cita
   del panel o cobro a un cliente nuevo) quedan sin prueba, y la Ley 1581 y RNF-LEG-01 la exigen.
   El backend debe recibir la aceptación y guardarla con el canal `panel`, que ya existe en la
   base.
5. **Hacer que la supresión borre también el celular del registro de mensajes.** Hoy la
   anonimización borra la ficha del cliente y revoca la autorización, pero
   `notificaciones.mensaje.destinatario` conserva el celular. La política promete borrarlo.
6. **Revisión de un abogado.** Esta versión cubre lo que exige el Decreto 1377 de 2013
   (artículo 13), pero la responsabilidad es del dueño del negocio.

## Cómo publicarla

1. El frontend muestra el texto en `/legal/tratamiento-de-datos`.
2. En el panel (o con `POST /api/v1/negocio/politicas`) se publica la versión:
   `{ "tipo": "tratamiento_datos", "version": "1.0", "url": "https://<dominio>/legal/tratamiento-de-datos" }`.
   Desde ese momento la reserva en línea funciona: exige el `id` de la política vigente, y cada
   autorización queda ligada a esa versión.
3. Si el texto cambia, se publica una versión nueva (1.1, 2.0…) con su fecha. Si cambian las
   finalidades, los clientes deben autorizar de nuevo.

## Texto de la casilla de autorización (formulario de reserva)

> Autorizo a Diamante Barbería a tratar mis datos personales para agendar y gestionar mis citas,
> enviarme por WhatsApp la confirmación, los recordatorios y los avisos de mis citas, hacer
> seguimiento del servicio y llevar sus estadísticas internas y su contabilidad, según su
> [Política de tratamiento de datos](/legal/tratamiento-de-datos). Mis datos pueden ser procesados
> por sus proveedores, como WhatsApp (Meta), fuera de Colombia.

La casilla es obligatoria y no puede venir marcada: el cliente debe marcarla él mismo. El enlace
abre la política en otra pestaña, sin perder lo que ya llenó.
