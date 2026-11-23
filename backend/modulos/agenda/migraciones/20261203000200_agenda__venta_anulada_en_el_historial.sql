-- Módulo `agenda` · fase 8: si se anula la venta que dejó una cita «Atendida», la cita vuelve a
-- «Confirmada» para cobrarla de nuevo, marcar la inasistencia o cancelarla; el historial lo
-- cuenta con este evento.
ALTER TYPE agenda.evento_cita ADD VALUE 'venta_anulada';
