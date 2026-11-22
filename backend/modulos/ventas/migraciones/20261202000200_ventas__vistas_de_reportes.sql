-- Módulo `ventas` · fase 6: vistas publicadas para `reportes` (contrato de lectura). Solo
-- incluyen ventas vigentes: una venta anulada sale de todos los totales (RF-VEN-12).

-- Servicios vendidos, con el precio vigente al vender (RN-18).
CREATE VIEW ventas.v_venta_linea AS
SELECT vl.venta_id,
       v.fecha_operacion,
       v.registrada_en,
       v.barbero_id,
       v.cliente_id,
       vl.posicion,
       vl.servicio_id,
       vl.precio
FROM ventas.venta_linea vl
JOIN ventas.venta v ON v.id = vl.venta_id
WHERE v.estado = 'registrada';

-- Lo cobrado por cada método de pago (propinas incluidas).
CREATE VIEW ventas.v_venta_pago AS
SELECT p.venta_id,
       v.fecha_operacion,
       v.barbero_id,
       m.codigo AS metodo,
       m.nombre AS metodo_nombre,
       p.monto
FROM ventas.venta_pago p
JOIN ventas.venta v ON v.id = p.venta_id
JOIN ventas.metodo_pago m ON m.id = p.metodo_pago_id
WHERE v.estado = 'registrada';
