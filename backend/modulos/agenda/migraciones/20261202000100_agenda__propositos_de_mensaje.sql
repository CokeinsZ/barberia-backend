-- Módulo `agenda` · fase 4: el recordatorio manual (RF-NOT-05) y el aviso para que el cliente
-- elija otra hora (RF-BAR-14) también quedan relacionados con su cita, para ver su estado en el
-- detalle y para atender las respuestas a los botones.
ALTER TYPE agenda.proposito_mensaje ADD VALUE 'recordatorio_manual';
ALTER TYPE agenda.proposito_mensaje ADD VALUE 'aviso_reprogramacion';
