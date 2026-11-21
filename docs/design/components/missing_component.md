# `missing_component` e blocchi sconosciuti

## Scopo
Due situazioni, un principio: **la lezione non deve rompersi, né far sentire l'utente in colpa o confuso, se un blocco non è disponibile**. Il Generator può dichiarare un desiderio di componente (`missing_component`); l'app, se incontra un `type` sconosciuto (catalogo più recente) o un blocco malformato, mostra un'alternativa neutra. Coerenza: l'utente non deve percepire "questo è un workaround".

## 1. `missing_component`

Rendering: **si disegnano in sequenza i blocchi di `fallback`**, ciascuno col proprio componente, come se fossero blocchi normali del documento (stessi gap, stessa colonna). **Nessun riquadro, nessun titolo, nessun messaggio** attorno; `wanted` e `purpose` **non sono mai mostrati all'utente**. Motivo: il fallback è scritto per essere una lezione completa; un'etichetta «componente mancante» sarebbe rumore (coerenza, Mayer) e darebbe un'impressione di prodotto rotto.

Regole:
- Gli esercizi in `fallback` sono esercizi veri: seguono `required`, entrano nello sblocco di «Continua» e nel tracking come gli altri (con `id` propri).
- Se un blocco del `fallback` non è renderizzabile, si salta quel blocco (non l'intero `missing_component`); se **nessuno** è renderizzabile, si mostra la riga neutra (§2).
- `missing_component` annidato in un fallback (vietato dal Validator): non si espande, riga neutra.
- Telemetria locale (solo dev/debug build): log `wanted` + `purpose`; in release niente. Il backend registra già la lista dei componenti desiderati alla generazione.

## 2. Riga neutra (`UnavailableBlock`)

Per: `type` sconosciuto senza `fallback`, blocco di tipo noto ma non renderizzabile (campi mancanti, riferimenti sconosciuti), `missing_component` senza fallback valido.

```
 (⊘)  Questo contenuto non è disponibile in questa versione dell'app.
```

| Misura | Valore |
|---|---|
| Layout | riga: icona `puzzle` 20 dp `inkMuted` + testo; gap 8 dp; allineata a sinistra, padding verticale 8 dp |
| Testo | `ui.body` (14/20) `inkSoft`; contrasto `inkSoft`/`bg` 9,0 / 10,9 |
| Sfondo/bordo | nessuno; **mai** `danger` o `warning` (non è un errore dell'utente né pericolo) |
| Interattività | nessuna (non selezionabile come azione; testo selezionabile) |
| Semantics | `contentDescription` = il testo; nessuna live region; ordine di lettura normale |

- **Non conta come esercizio obbligatorio** (altrimenti bloccherebbe la lezione): né ai fini dello sblocco di «Continua», né del riepilogo.
- Una sezione che contiene solo righe neutre si sblocca da sola.
- Più righe neutre consecutive si **fondono** in una sola riga (non ripetere il messaggio).
- Se **tutti** i blocchi di una sezione sono neutri: la sezione resta nell'indice con il suo titolo.
- Questo vale anche per un blocco il cui `type` è noto ma la cui `catalog_version` del documento è superiore: tolleranza totale (contratto §1).

## Accessibilità, stati, contenuti al limite
Statico, nessuno stato. Font 200 %: il testo va a capo accanto all'icona. Lunghezza fissa.

## Linee guida per il Generator
Vedi README §11. In sintesi: usare `missing_component` **solo** se esiste un bisogno didattico reale che nessun blocco del catalogo copre (diagramma, tabella comparativa, linea del tempo, passo-passo animato); `wanted` in `snake_case` e `purpose` che descrive la **funzione didattica**; `fallback` 1-6 blocchi **sufficienti da soli** (un diagramma → un `code` `text` con disegno ASCII ≤ 60 colonne e il paragrafo che lo commenta; una tabella → paragrafi brevi); massimo 2 per lezione; mai annidati, mai l'unico contenuto di una sezione, mai per aggirare un limite di lunghezza.
