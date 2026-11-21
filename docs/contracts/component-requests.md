# Contratto: Componenti mancanti

> Contratto condiviso tra backend e app. Proprietario: tech lead. Versione: **v1.1** (2026-11-10).
> **v1.1:** la classifica conta solo le richieste nate dagli argomenti dell'utente della sessione.
> Valgono `api-conventions.md`, `sdui.md` (blocco `missing_component`) e `lessons.md` §4 (al salvataggio di una lezione nasce una `ComponentRequest` per ogni blocco `missing_component`). Flusso: `docs/mvp.md` §7.6. I path omettono il prefisso `/api/v1`.

## 1. Concetti

- Quando il Generator vorrebbe un componente che il catalogo non ha, scrive un blocco `missing_component` (`wanted`, `purpose`, `fallback`). L'app mostra il fallback, il backend registra la richiesta.
- La classifica serve al proprietario per decidere quale componente implementare nel prossimo catalogo. Raggruppamento per **nome esatto** di `wanted` (nessun clustering semantico nell'MVP).

## 2. Endpoint

| Metodo | Path | Successo | Errori |
|---|---|---|---|
| GET | `/component-requests` | `200` lista paginata di `ComponentRequestGroup` | `400 bad_request` |

Ordine: `count` decrescente, poi `last_requested_at` decrescente, poi `wanted` crescente. Paginazione standard (`api-conventions.md` §8).

### `ComponentRequestGroup`

```json
{
  "wanted": "interactive_timeline",
  "count": 4,
  "first_requested_at": "2026-11-09T15:02:11.420Z",
  "last_requested_at": "2026-11-12T08:40:01.003Z",
  "catalog_version": 1,
  "examples": [
    {
      "purpose": "Mostrare l'ordine degli eventi della vita di un valore",
      "lesson_id": "01933b4e-...",
      "lesson_title": "Ownership e move",
      "topic_title": "Rust",
      "requested_at": "2026-11-12T08:40:01.003Z"
    }
  ]
}
```

| Campo | Regole |
|---|---|
| `wanted` | come nel blocco (`snake_case`, 1-60) |
| `count` | numero di blocchi `missing_component` con quel `wanted` in tutte le lezioni |
| `catalog_version` | la versione più alta del catalogo con cui è stato chiesto (se il componente arriva in un catalogo nuovo, le richieste vecchie restano leggibili) |
| `examples` | al massimo 3, i più recenti; `purpose` come nel blocco |

Le richieste delle lezioni eliminate spariscono con la lezione (cascata).
