# ADR 0017 — Tracing con OpenTelemetry e Jaeger

- Stato: accettata
- Data: 2026-11-20
- Richiesta «tracing OpenTelemetry + Jaeger». Il numero 0014 era già occupato da ADR 0014 (FSRS-7), quindi questa ADR è la 0017.

## Problema

Fino ad ora il backend scrive solo log con `tracing`. Per capire dove si perde tempo in una richiesta o in un job (chiamate LLM, sintesi vocale, accesso al database) serve una vista per tracce, con i tempi di ogni passo. I log non la danno.

## Scelta

1. **Librerie:** `tracing-opentelemetry` e `opentelemetry-otlp` 0.33 con trasporto HTTP/protobuf. I log `tracing` restano invariati.
2. **Attivazione opt-in:** l'export parte solo se è impostato `OTEL_EXPORTER_OTLP_ENDPOINT` oppure `[telemetry] otlp_endpoint` nella configurazione. Con valore vuoto l'export è disattivato. Il nome del servizio è `service.name=teach-me`.
3. **Span:** richieste HTTP con route, job, chiamate LLM con conteggio dei token, sintesi vocale (TTS).
4. **Dati esclusi:** mai corpi di richiesta o risposta, mai header, mai chiavi, mai testi dell'utente.
5. **Jaeger v2 all-in-one nel compose:** tracce solo in memoria, massimo 5000 tracce, limite di 512 MB. L'interfaccia web è esposta solo su loopback (127.0.0.1:16686), salvo cambio con `JAEGER_UI_BIND_IP`.

## Alternative scartate

- **Solo log `tracing` con più dettaglio:** nessuna vista dei tempi tra i passi di una stessa richiesta.
- **Jaeger con archiviazione persistente (database o Elasticsearch):** più risorse sul server di casa per un beneficio che al momento non serve.

## Conseguenze

- Le tracce si perdono al riavvio di Jaeger, perché l'archiviazione è in memoria.
- L'export si spegne svuotando la variabile, senza ricompilare.
- Un servizio in più nel compose, limitato da memoria e numero di tracce.
- Da verificare col deploy: che l'export non rallenti le richieste e che nessun dato sensibile finisca negli span.
