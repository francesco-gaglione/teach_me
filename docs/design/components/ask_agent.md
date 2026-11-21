# `ask_agent`

> Fase 7: il tocco apre davvero la Chat del Topic (contratto `lessons.md` §7: `lesson_id`). Vale anche per «Chiedi al Tutor» del Riepilogo (`../screens/lesson.md` §7) e «Ne parlo col Tutor» di `open_answer.md`.

## Scopo didattico
Dare **un'uscita verso il dialogo** quando la lezione non basta (metacognizione: «non ho capito» è un segnale da sostenere, non da nascondere). Riduce la frizione: il composer della chat si apre già con una domanda sensata sul punto giusto, e il Tutor sa di quale lezione si parla (contiguità: la domanda non perde il suo contesto).

## Anatomia

```
┌──────────────────────────────────────────────────┐ ListRow in card statica (72 dp)
│ (💬)  Hai dubbi? Chiedi al Tutor              ›  │ lead icona message-circle 24 accent ; label ui.title3 ink ; chevron-right
│       Apre la chat con una domanda già pronta    │ sottotitolo ui.body inkSoft (fisso)
└──────────────────────────────────────────────────┘
```

- `label` = campo `label` o, se assente, «Hai dubbi? Chiedi al Tutor».
- Posizione: sempre nell'ultima sezione, prima di «Completa lezione». Gap 24 dp.
- Non conta come esercizio, non cambia il progresso, non è mai obbligatorio.

## Cosa succede al tocco

1. **Si salva lo stato della lezione** (sezioni, bozze, posizione di scroll: `lesson.md` §2).
2. **Si apre l'argomento sulla tab Chat** (la schermata Lezione **resta nello stack**, sotto: *non* si chiude), con:
   - il composer **precompilato** con `prompt_hint` (modificabile, **mai inviato da solo**), cursore **in fondo** al testo, focus sul composer; la tastiera virtuale si apre (non con una tastiera fisica collegata, che non la mostra);
   - la lezione come **contesto**: un `LessonContextChip` sopra il campo (sotto);
   - la lista dei messaggi in fondo (ultimi messaggi).
3. L'utente modifica, invia (Invio con tastiera fisica / pulsante) o rinuncia. All'invio la richiesta porta `lesson_id` (`POST /topics/{id}/messages`, `lessons.md` §7) **una sola volta** (per quel turno); il chip sparisce all'invio.
4. **Tornare alla lezione**: il tasto Indietro (✕/Indietro del sistema; Alt+←) ritorna alla lezione **nella posizione salvata**, anche mentre il Tutor sta rispondendo (lo stream continua sul backend; la risposta sarà nella chat al prossimo accesso). Nessun dialog di uscita (nulla si perde: la bozza del composer si conserva, `chat.md` §8).

### `LessonContextChip` (nuovo, piccolo)

Riga sopra il campo del composer, `ui.labelS` `ink`, fill `surfaceSunken`, raggio `full`, altezza visiva 32 dp, icona `book-open` 16 `inkSoft` a sinistra: «Sulla lezione 3: Lifetimes in pratica» (una riga, ellissi a font 1×; a capo a font grande) + IconButton `x` 48 dp (area di tocco) «Togli la lezione dal contesto» a destra. Rimuoverlo = il messaggio parte **senza** `lesson_id` (domanda generale). Contrasti: `ink`/`surfaceSunken` 14,95 / 15,00. È una frase d'interfaccia, non decorazione: dice che cosa sa il Tutor di questa domanda, e permette di toglierlo (trasparenza e controllo, P9).

### Composer già con una bozza

Se il composer contiene già testo non inviato (bozza), **non si sovrascrive**: `prompt_hint` si aggiunge in fondo dopo una riga vuota, e il cursore va in fondo. Nessun avviso.

### Tablet vs telefono

| | Expanded (≥ 1100 dp) | Expanded stretto / medium / compact |
|---|---|---|
| Layout della Chat | colonna chat a 600 dp + `ContextPane` 280 dp (`chat.md` §2) | colonna sola |
| Dove si vede la lezione | in cima al `ContextPane`, sopra la sintesi della mission, un blocco **«Lezione»**: titolo (`ui.title3`) + «Lezione 3 · Circa 8 minuti» (`ui.bodyS` `inkSoft`) + Button secondario **«Torna alla lezione»** (48 dp). Resta finché il chip è attivo o la chat è stata aperta dalla lezione in questa visita | nessun pannello: Indietro (chevron-left con `contentDescription` «Torna alla lezione «Lifetimes in pratica»», solo in questa visita) |
| Tastiera | tastiera fisica frequente: il composer ha il focus, Invio invia, Esc non fa nulla; Alt+← torna alla lezione | tastiera virtuale: si apre subito; il campo sopra l'IME (`imePadding`) |

*Perché non una chat affiancata alla lezione* (split view): richiederebbe due colonne di testo di studio contemporanee (ADR 0003 §2: «lo spazio laterale è contesto, non testo più largo») e sposterebbe l'attenzione continuamente. Il `prompt_hint` cita già il punto; la lezione si ritrova con un tocco. Da rivalutare dopo i test d'uso (Aperto).

### Edge case

- **Tutor offline / backend irraggiungibile**: l'azione funziona comunque (il composer si apre, il testo resta; l'invio mostra gli errori standard di `chat.md` §9.3).
- **Turno in corso del Tutor**: il composer è attivo con la bozza, «Invia» è disabilitato con il motivo di `chat.md` §8.
- **`422` con `field: "lesson_id"`** (lezione non più nel Topic): l'app **reinvia senza `lesson_id`** in silenzio: il contesto è un di più, mai una ragione di fallire.
- **Lezione `completed` o in revisione**: funziona uguale.
- **Topic non `ready`**: l'`ask_agent` esiste solo in lezioni, quindi il Topic è `ready`.
- **Doppio tocco**: la navigazione è idempotente (una sola Chat in cima).

## Stati
Normale, premuto, focus. Nessuno stato disabilitato.

## Accessibilità
`Role.Button`, «<label>. Apre la chat con il Tutor». All'arrivo nella Chat: annuncio Polite «Domanda precompilata nel campo messaggio. Modificala e inviala quando vuoi.»; il focus di accessibilità va al campo. Il chip è un nodo («Contesto: lezione 3, Lifetimes in pratica») con azione «Togli». Tastiera: Alt+← torna alla lezione; F6 passa fra colonna e pannello.

## Contenuti al limite
`prompt_hint` 300 caratteri: il composer lo mostra a più righe (cresce fino al 40 % dell'altezza utile, poi scorre); `label` 80 va a capo; titolo di lezione lungo nel chip: a capo.

## Linee guida per il Generator
Almeno **uno** (`ask_agent_missing`), in chiusura. `prompt_hint` = una **domanda concreta e specifica** sul punto più probabile di dubbio, in prima persona, già formulata come la scriverebbe l'utente («Non ho capito perché dopo `let b = a;` non posso più usare `a`: me lo spieghi con un altro esempio?»); mai generico («Ho un dubbio»). Visto che il Tutor riceve titolo, obiettivo e numero della lezione (non il documento), la domanda deve bastare **da sola** a capire il punto: nominare il termine o il passaggio. `label` personalizzata solo se serve: il default è già buono.
