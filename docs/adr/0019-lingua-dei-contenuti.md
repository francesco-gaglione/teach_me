# ADR 0019: Lingua dei contenuti

- Stato: accettata
- Data: 2026-11-21
- Issue Gitea #30 ("supporto all'inglese"). Decisione del tech lead. Contratti toccati: `docs/contracts/folders-topics.md`, `docs/contracts/api-conventions.md` §7, `docs/contracts/sdui.md` §2 e schema `sdui-schema-v1.json`, `docs/contracts/lessons.md` §3.1.

## Problema

L'app e tutti i contenuti generati dagli agenti sono oggi solo in italiano: le lezioni sono in italiano con termini originali (ADR 0005, D3). Serve supportare l'inglese senza rendere incoerenti gli argomenti già avviati. Bisogna decidere a chi appartiene la lingua: all'utente, all'argomento o al dispositivo.

## Scelta

1. **La lingua dei contenuti è una proprietà dell'argomento** (`Topic.language`), scelta alla creazione e immutabile. Valori ammessi: `"it"`, `"en"` (ISO 639-1 minuscolo). Default se assente nella richiesta: `"it"`. Gli argomenti esistenti diventano `"it"` (migrazione).
2. **Tutto ciò che gli agenti producono per un argomento è nella sua lingua**: chat del Tutor, mission, annotazioni del Researcher, piano, lezioni, feedback del Grader, record, glossario e schede del Recorder, lettura ad alta voce (voce TTS nella lingua dell'argomento). Le citazioni testuali da fonti in un'altra lingua restano nella lingua originale, con `lang`.
3. **Nei documenti SDUI il campo `lang` di un blocco si usa solo quando il contenuto non è nella lingua dell'argomento** (prima: "non è italiano"). Il documento della lezione non cambia forma.
4. **I prompt restano scritti in italiano** e ricevono la lingua d'uscita con il segnaposto `{{content_language}}` (valori "italiano" / "inglese"), reso dal Context Assembler per ogni ruolo.
5. **Testi mostrati alla persona**: quelli che appartengono a un argomento (i testi degli avvisi di qualità della lezione, `quality_warnings` in `lessons.md`) sono nella lingua dell'argomento. I `message` degli errori API seguono l'header `Accept-Language` della richiesta: `en` se la prima lingua supportata è l'inglese, altrimenti italiano (default). I messaggi di riparazione mandati al modello (errori del Validator) restano in italiano: sono istruzioni interne, mai mostrate.
6. **Interfaccia Android**: segue la lingua del dispositivo (risorse `values/` in italiano, `values-en/` in inglese). Alla creazione di un argomento l'app propone la lingua del dispositivo se supportata, altrimenti `"it"`; la persona può cambiarla.
7. **Evals**: il controllo deterministico `language` accetta `"it"` ed `"en"`; ogni ruolo ha anche casi in inglese.

## Alternative scartate

- **Lingua per utente**: cambiarla renderebbe incoerenti gli argomenti già avviati.
- **Lingua implicita del dispositivo**: nessun controllo per argomento; un argomento cambierebbe lingua con il telefono.
- **Prompt duplicati per lingua**: doppia manutenzione di ogni prompt.

## Conseguenze

- Migrazione `topics.language` (valore `"it"` per gli argomenti esistenti).
- Campo `language` nelle risposte e nella creazione dell'argomento (`docs/contracts/folders-topics.md`).
- Header `Accept-Language` per i `message` degli errori (`docs/contracts/api-conventions.md` §7).
- Segnaposto `{{content_language}}` nei prompt, reso dal Context Assembler.
- Voce TTS scelta per lingua (`ADR 0013`).
- Risorse `values-en/` in tutti i moduli Android.
- Campo `language` di sola lettura in `LessonView` (`docs/contracts/lessons.md` §3.1, v1.9): lingua dell'argomento per i blocchi SDUI senza `lang`.
