# `checklist`

## Scopo didattico
**Trasferimento al mondo reale** (implementation intentions): trasforma ciò che si è capito in 2-5 azioni concrete da fare *fuori* dall'app. È **autodichiarata**: nessuna correzione, nessun obbligo, nessun punteggio (P9). Non è un esercizio di richiamo e non compare mai come "da fare" nel progresso della lezione.

## Anatomia

```
┌──────────────────────────────────────────────────┐ Surface static (fill surface, bordo line, raggio md, padding 16)
│ (✓) Prova tu                                      │ icona list-checks 20 inkSoft + title: ui.title3 ink (se assente: «Da fare»)
│ ┌──────────────────────────────────────────────┐ │
│ │ ☐  Scrivi un esempio in cui `a` viene spostata│ │ ChoiceRow checkbox (56 dp min), gap 8 dp
│ └──────────────────────────────────────────────┘ │
│ ┌──────────────────────────────────────────────┐ │
│ │ ☑  Prova a compilarlo e leggi l'errore       │ │ selezionata: fill accentSoft, bordo accent 2, spunta
│ └──────────────────────────────────────────────┘ │
│ 1 di 2 fatte                                      │ ui.bodyS inkSoft
└──────────────────────────────────────────────────┘
```

- Voce = `ChoiceRow` (`base-components` §9) variante checkbox, `ui.bodyL`, md inline `interactive = false`. **Nessuna** marcatura "giusto/sbagliato", **nessun** pannello di esito, nessun bottone «Controlla». Il toggle è immediato (`selectionChange` 100 ms) e salvato per `item.id`.
- Contatore «k di n fatte» in `ui.bodyS` `inkSoft` (testo, non barra: non è un obiettivo da completare).
- Non cambia l'avanzamento della lezione né sblocca/blocca nulla. Non ha `required`.
- Stati: default, premuto, selezionata, focus (anello), disabilitata mai.

## Interazione
Tocco e **Spazio** alternano; frecce ↑↓ spostano il focus (un gruppo); numeri 1-8 alternano la voce N (con `KeyHint` se tastiera fisica).

## Accessibilità
`Role.Checkbox`, `toggleableState`, `selectableGroup`; titolo come heading di gruppo; «k di n fatte» `liveRegion = Polite` a ogni cambiamento. Sulla lettura: «Prova a compilarlo, selezionato, 2 di 2».

## Contenuti al limite
8 voci da 200 caratteri: ≈ 8 × 90 dp = 720 dp, ok. Titolo 80 caratteri a capo. 1 sola voce: ammesso (contatore «0 di 1 fatta»).

## Linee guida per il Generator
Vedi README §10: azioni **fuori dall'app**, al verbo, osservabili («Scrivi…», «Esegui…», «Leggi la sezione 4.1 del libro»), 2-5 voci, ognuna fattibile in ≤ 10 minuti. Mai per ripetere concetti, mai come sostituto di un esercizio, mai l'unico esercizio della lezione. Tipicamente nella sezione finale, prima di `primary_source`. Titolo opzionale: «Prova tu», «Per consolidare».
