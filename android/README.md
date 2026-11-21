# teach_me - Android app

Kotlin, Jetpack Compose on `androidx.compose.foundation` (custom design system, no Material), Hilt (KSP),
Ktor (OkHttp engine) + kotlinx.serialization, Navigation Compose (type-safe), DataStore.

## Build

The build needs **JDK 21** (the system JDK may be newer). Do not commit machine paths:

```
export JAVA_HOME=/path/to/jdk-21          # e.g. ~/.jdks/ms-21.0.12.1
echo "sdk.dir=/path/to/Android/Sdk" > local.properties   # git-ignored
./gradlew ktlintCheck lint testDebugUnitTest assembleDebug
```

Versions are pinned in `gradle/libs.versions.toml`; the Gradle wrapper pins Gradle.
`compileSdk` is 37 (current AndroidX/OkHttp require it), `targetSdk` 35, `minSdk` 29.

## Modules

| Module | Role |
|---|---|
| `app` | Application, MainActivity, navigation, expanded list-detail container (`LibraryScreen`); `reminders/`: hourly WorkManager check of due review cards, notification, scheduler (ADR 0012) |
| `core:designsystem` | `TeachTheme` + base components + Lucide icons + fonts |
| `core:model` | Pure Kotlin: session and user scope, LLM key state, health, API error, folder/topic models and repository interfaces |
| `core:network` | Ktor client, `HealthApi`, `FolderApi`/`TopicApi`, error parsing |
| `core:data` | Session (`SessionRepository`, token wrapped by the Android Keystore), per-user queues, LLM key monitor, `LibraryStateRepository`; folder/topic repositories; flash inbox; last notified review count per user |
| `core:ui` | Shared UI of the feature modules: dialog wiring, error mapping texts, topic state chip |
| `sdui` | Lesson renderer: block registry and native components |
| `feature:login` | Sign-in screen (address, user name, password) |
| `feature:settings` | Settings: account, sign out, LLM key (write only), change password, connection test |
| `feature:home` | Folder list pane (stacked Home or left pane of the list-detail layout) |
| `feature:folder` | Folder (topics) pane and minimal Topic screen |

Convention plugins live in `build-logic/` (`teachme.android.application|library|compose|hilt|feature`,
`teachme.kotlin.library`).

## Review notifications (ADR 0012)

- **Check.** `ReviewAlerts.kt` in `core:model` holds the pure alert rule (`ReviewAlertRule`) and `ReviewReminderCheck`
  (`GET /reviews/due?limit=1`, rule, notifier, stored reference). `app/.../reminders/` has the `@HiltWorker`
  (`ReviewReminderWorker`, hourly, network constraint, `KEEP`), the notification (`AndroidReviewNotifier`, channel
  "Ripassi", fixed id) and `WorkManagerReviewReminderScheduler` (`schedule()`/`cancel()`; `cancel` also clears the
  notification). WorkManager starts on demand (Hilt factory in `TeachMeApplication`).
  The check waits for the stored session (`awaitReady`): a cold worker never reads `Loading` as "signed out". The worker
  first asks `ReviewReminderGate.mayRun()` (switch on and permission): otherwise it cancels itself, with no network.
  `ReviewNotifier.show` returns false when blocked, and then the stored reference is left alone; a silent update (no
  growth) only touches a notification still on screen (`setSilent`), so a dismissed one does not come back with sound.
- **Who decides.** `ReviewReminders.kt` in `core:model`: `ReviewReminderGate` is the only place that schedules or cancels.
  It runs when the session changes (`MainViewModel` collects `sessions.state`: signed in, then `sync()`; sign-out or a
  `401`, then `cancel()`), at every foreground return, from the Settings switch, and after the permission dialog.
  `sync()` schedules only with the switch on and the permission granted.
- **Switch.** "Notifiche dei ripassi" in Settings (`feature:settings`, `NotificationsCard`): on by default, stored only
  on the device (`ReviewReminderPrefs`, DataStore `review_reminders`, not per user). Turning it on without the permission
  asks for it; when the permission is missing the card says so and opens the system notification settings.
- **Permission.** `NotificationPermissionAsker` (`core:model`): asks from the switch, and once from the Home "Ripasso di
  oggi" card the first time it shows due cards (flag `permissionAsked`; any ask uses it up; never at start; not with the
  switch off). The dialog is `rememberNotificationPermissionRequest` in `core:ui` (no dialog before Android 13). "Granted" means the runtime
  permission, `areNotificationsEnabled()` and a "Ripassi" channel not at `IMPORTANCE_NONE`. The Home ask is marked as used
  only when the screen launches the dialog (`HomeViewModel.onNotificationPermissionRequested`).
- **Tap.** `ReviewIntents.openReview` (action `OPEN_REVIEW`, single top). `MainActivity` reads it at the first start and in
  `onNewIntent`, and keeps a pending request in `onSaveInstanceState` (a rotation neither loses nor repeats it); `TeachMeApp` navigates to the existing `ReviewRoute` once signed in (a pending tap waits for the login).

## Read aloud (ADR 0013, `docs/contracts/audio.md`)

- **Data.** `LessonAudio.kt` in `core:model` (manifest, voices, `AudioRepository`, `ClipPlayer` interface); `AudioApi` in
  `core:network`; `DefaultAudioRepository` in `core:data`. An unknown `status` reads as `pending`.
- **State.** `LessonAudioViewModel` (`feature:lesson`, a second `hiltViewModel()` of the lesson screen, same `key`): asks
  `GET /tts/voices` (feature off: nothing else is asked, no control is shown), then the manifest, again every 15 s
  while a block is `pending`; polling ends with the screen. Per block: `BlockListenState` (`Playable` / `Preparing` /
  `Unavailable`); blocks outside the manifest have no button. Sample (`fixture/`) lessons have no audio.
- **Player.** `Media3ClipPlayer` (`core:data`; ExoPlayer + OkHttp data source, no redirects) behind `ClipPlayer`; created on the
  first play, released in `onCleared`. The clip URL is resolved against the stored base URL + `/api/v1` with the same
  Bearer token. Reading stops on ON_STOP, when the screen is disposed and when another block starts.
- **Per-block control** (`lesson-audio.md` sections 1, 2). `AudioListenable(block)` (`LessonAudioUi.kt`): icon-only
  `volume-2` in `inkSoft`, 48 dp; on a row below paragraphs, callouts and diagrams (the gap to the next block gives up
  16 dp, see `hasControlRowBelow`), inline at the end of headings. Playing: `square` + "Ferma" in `accent`, name "Ferma la
  lettura", `stateDescription` "In lettura". Preparing: `clock`, disabled. Unavailable: `minus-circle` + words, disabled.
  The now-playing mark is one 4 dp `accent` bar drawn in the margin (-12 to -8 dp, `drawBehind`, no fill, no reflow),
  fading in `fast`.
- **"Ascolta la lezione".** `volume-2` in `LessonTopBar` (`LessonListenAction`, left of the index; disabled "in
  preparazione" until a block has a clip; absent if the feature is off). `LessonAudioViewModel.listenToLesson(startId,
  slots)` reads in document order from the first readable block at the top (`firstSlotAtOrAfter`). `readingSlots`
  (`ReadingPlan.kt`) turns the revealed rows into `Read` / `Gate` slots and is asked again at every step: blocks without a
  clip are skipped, exercises are never read, a required unanswered exercise is a `Gate` where the reading stops
  (`LessonAudioEvent.StoppedAtExercise`: the screen scrolls to it and shows the toast). Reaching the end emits `Finished`
  (the page goes to "Continua" if hidden). Auto-scroll (`Reading` events) happens only when the block is not fully
  visible (`isFullyVisible`); a drag by the user suspends it until the reading ends.
- **`AudioStrip`** under the top bar while the continuous reading runs: "Ferma la lettura" and the speed chip.
- **Speed.** `PlaybackSpeed` (0,75 / 1 / 1,25 / 1,5) and `AudioPreferences` (a Flow) in `core:model`;
  `DataStoreAudioPreferences` in `core:data` keeps it in the device `settings` DataStore (key `audio_speed`). The chip
  cycles it; `ClipPlayer.setSpeed` sets Media3 `PlaybackParameters(speed, 1f)` at once. Applies to both reading modes.
- **Settings "Ascolto"** (`lesson-audio.md` section 4). `ListeningViewModel` + `ListeningSection.kt` in `feature:settings`, a
  second `hiltViewModel()` of `SettingsRoute`. Between "Notifiche" and "Connessione"; not drawn at all while the voice
  list has not arrived, when `available: false` or when `GET /tts/voices` fails (asked again at every ON_START). Speed:
  `TeachSegmentedSwitch` (now with per-segment descriptions and a `stacked` column mode, used from font scale 1,3) bound
  to `AudioPreferences`, saved at the tap. Voice: radio rows of 56 dp; a tap is optimistic (`AuthApi.setTtsVoice` =
  `PUT /me/tts-voice`, answer stored through `SessionRepository.updateUser`; `AuthUser.ttsVoice` is nullable and
  persisted); on 422/409/network error the radio goes back and a toast shows (a 401 stays silent; 422 and 409 also reload
  the list). "Prova" plays `sample_url` through its own `ClipPlayer` at 1x (never the stored speed), one at a time, stops
  on ON_STOP and is released in `onCleared`; `sample_url: null` is "In preparazione", a playback error is "Non
  disponibile" until the next start. `Media3ClipPlayer` and its Hilt binding now live in `core:data` (shared by lesson and
  settings).

## Design tokens

`docs/design/tokens.json` is the source of truth. `core/designsystem/.../GeneratedTokens.kt` is generated:

```
python3 -I tools/generate_tokens.py
```

`TokensMatchJsonTest` fails when the file is stale. Icons are imported with
`python3 -I tools/import_lucide_icons.py`.

## Cleartext HTTP

`app/src/main/res/xml/network_security_config.xml` allows `http://` on purpose: the backend is reached
over Tailscale/LAN, usually without TLS (see ADR 0001). Remove it if the backend is ever exposed publicly.

## Release build

```
./gradlew assembleRelease          # app/build/outputs/apk/release/
```

R8 (minify + resource shrinking) is on. `app/proguard-rules.pro` keeps the generated kotlinx.serialization serializers
of the app's own classes (SDUI blocks, API DTOs, routes); the libraries ship their own consumer rules. Debug-only
tools (sample lesson fixtures, the "lezioni di esempio" section in Settings) are not in the release APK.

### Signing

The release build reads `android/keystore.properties` (git-ignored, see `keystore.properties.example`; keys
`storeFile`, `storePassword`, `keyAlias`, `keyPassword`; `storeFile` is relative to `android/` or absolute).
**Without that file the build still succeeds but the APK is unsigned** (`app-release-unsigned.apk`, not installable)
and Gradle prints a warning. Never commit the keystore or the properties file.

Create a keystore once (keep a backup: with a new key the app cannot be updated, only reinstalled):

```
keytool -genkeypair -v -keystore teachme-release.jks -alias teachme \
  -keyalg RSA -keysize 4096 -validity 10000
cp keystore.properties.example keystore.properties   # then fill in the passwords
./gradlew assembleRelease                            # -> app-release.apk, signed
adb install -r app/build/outputs/apk/release/app-release.apk
```

Check the signature with `apksigner verify --print-certs app/build/outputs/apk/release/app-release.apk`.

The backend is reached over `http://<tailscale-ip>:8080`; cleartext is allowed by the network security config in
release too (see below).

## Third-party licenses

The list shown in Settings > "Licenze open source" is `feature/settings/src/main/assets/licenses.json`, kept by hand:
update it when `gradle/libs.versions.toml` changes.


- Icons: [Lucide](https://lucide.dev), ISC License, Copyright (c) Lucide Icons and Contributors.
  Each imported drawable carries the notice.
- Fonts (SIL Open Font License 1.1, from Google Fonts, variable TTF in `res/font`): Inter, Literata,
  JetBrains Mono.
- Libraries: Apache 2.0 (AndroidX incl. Media3, Ktor, OkHttp, Dagger/Hilt, kotlinx).
