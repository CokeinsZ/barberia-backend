-- Módulo `ventas` · fase 8: una cita cuya venta se anuló se puede volver a cobrar (p. ej., se
-- anuló para corregir el servicio). La cita sigue sin poder tener dos ventas vigentes: la
-- unicidad queda solo entre las ventas registradas (las anuladas se conservan, RNF-INT-06).
ALTER TABLE ventas.venta DROP CONSTRAINT venta_cita_uk;
CREATE UNIQUE INDEX venta_cita_vigente_uk ON ventas.venta (cita_id) WHERE estado = 'registrada';
