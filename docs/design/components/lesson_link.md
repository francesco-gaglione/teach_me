# `lesson_link`

## Scopo didattico
Collegare la lezione alle altre dello stesso argomento (**interleaving e contesto**: l'utente vede come si inserisce nel percorso e può proseguire quando vuole). È un'offerta, mai un obbligo; non gestisce l'ordine delle lezioni (lo fa la Mission).

## Anatomia

```
┌──────────────────────────────────────────────────┐ Surface interactive: ListRow in card statica
│ (→)  Continua con il borrowing               ›   │ lead icona arrow-right-circle 24 inkSoft ; label ui.title3 ink ; chevron-right 24 inkSoft
│      Lezione dell'argomento «Rust»               │ sottotitolo ui.body inkSoft (titolo dell'argomento)
└──────────────────────────────────────────────────┘
```

- `ListRow` di `base-components` §4 (72 dp con sottotitolo), dentro una card statica. Un unico target. Tocco → naviga alla lezione `lesson_id` **dopo aver salvato lo stato** della corrente (nessun dialog); Indietro torna alla lezione di partenza nella posizione salvata.
- `label` (≤ 120) è il testo scritto dal Generator; il sottotitolo è fornito dall'app (titolo dell'argomento).
- **Lezione non disponibile** (id sconosciuto, lezione eliminata, non ancora generata): la riga è visibile ma neutra (`disabledInk`, nessun chevron) con sottotitolo «Questa lezione non è ancora disponibile.»; tocco non fa nulla, annuncio TalkBack come tale. Non è un errore (nessun rosso).
- Gap 24 dp. Si impilano più `lesson_link` come righe distinte con gap 8 dp (non in un'unica card).

## Stati
Normale, premuto (`surfacePressed`), focus, non disponibile (sopra).

## Accessibilità
`Role.Button`, descrizione «<label>. Lezione dell'argomento Rust. Apre un'altra lezione». Non disponibile: `stateDescription` «Non ancora disponibile».

## Contenuti al limite
`label` 120 caratteri: va a capo. Più di 3 `lesson_link` nella stessa sezione: si disegnano tutti, il Generator non dovrebbe.

## Linee guida per il Generator
Solo nella sezione finale, 0-2 per lezione, e solo verso lezioni **già esistenti** nel Topic (`invalid_lesson_link`). `label` che dica **cosa si farà** («Continua con il borrowing»), non «Prossima lezione» né «Clicca qui». Non usarlo per ripetere prerequisiti dentro la lezione (i prerequisiti sono nel Topic).
