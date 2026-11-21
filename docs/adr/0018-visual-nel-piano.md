# ADR 0018: Campo `visual` nel piano, per far comparire figure e componenti mancanti

- Stato: accettata
- Data: 2026-11-20
- Decisione del tech lead, approvata dal proprietario il 2026-11-20. Il numero 0015 è già preso da ADR 0015 (scrittura a mano), quindi questa è 0018. Il piano del Planner è descritto in `docs/mvp.md`; i valori del blocco `diagram` e di `missing_component` in `docs/contracts/sdui.md` e `docs/contracts/lessons.md`. Nessuna modifica a questi contratti.

## Problema

Le lezioni reali non mostrano figure: 0 blocchi `diagram` in 15 lezioni reali, e circa 105 nelle eval. Iterare solo sul prompt del Generator (v15-v19) ha portato la quota di lezioni con figura a circa due terzi, senza raggiungere l'80%. La causa è che il piano non decide quale figura serve a ciascun concetto: il Generator scrive il documento senza pianificare la figura, e la figura resta un caso casuale.

## Scelta

1. **Campo `concepts[].visual` nel piano**, facoltativo. Assente equivale a `none`.
2. **Valori ammessi**:
   - `none`, `flow`, `tree`, `cycle`: il Generator deve produrre un blocco `diagram` con quel `layout`;
   - `code_compare`, `line_chart`, `comparison_table`, `timeline`, `sequence`: il Generator deve produrre un `missing_component` con quel `wanted`.
3. **Massimo due valori diversi da `none`** in tutto il piano.
4. **Valore presente ma non canonico**: il piano non è valido.
5. **Il Generator riceve la riga `Figura: <valore>`** subito dopo «Errore tipico» nella scheda del concetto.
6. **Controllo interno `planned_visual`** (`backend/crates/application/src/planned_visual.rs`):
   - il documento deve avere almeno tanti blocchi di primo livello per ogni valore richiesto, cioè per valore il minimo tra richiesti e presenti;
   - se manca qualche figura, si fa il passaggio di miglioramento (`improve`) già esistente, con un punto in più da correggere;
   - il documento migliorato si tiene solo se è valido, non perde le figure già soddisfatte e non peggiora;
   - il controllo rispetta il limite di riparazioni già previsto.
7. **Fuori da `quality_warnings`**: il controllo non produce avvisi visibili. Nessuna modifica di contratto, nessuna migrazione.
8. **Versioni dei prompt**: planner `planner-phase12-v5`, generator `generator-visual-v20`.

## Alternative scartate

- **Continuare a iterare solo sul prompt del Generator**: ha già dato circa due terzi senza arrivare all'80%. Il Generator non può decidere la figura se il piano non la prevede.

## Conseguenze

- I piani già salvati restano validi: senza `visual` valgono come `none`.
- Quando la figura manca, si paga un passaggio `improve` in più.
- La qualità didattica delle figure (posizione nella lezione, ridondanza con il testo) non è risolta da questa decisione e resta da migliorare.
- Il dato su `missing_component` raro in `docs/design/review-mvp.md` e `docs/design/quality-warnings.md` non è aggiornato qui: l'area è di `learning-designer`.
