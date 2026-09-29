---
name: mobile-testing
description: "Testing mobile apps: unit tests for logic, component or widget tests, a few end-to-end flows with Maestro, Detox, XCUITest or Espresso, a device matrix (small screens, large text, dark mode, right-to-left, old OS versions), network conditions, permissions, deep links and push notifications, accessibility audits, and snapshot tests. Read before writing or fixing tests for a mobile app."
---

# Testing mobile apps

The naive mobile test suite is either empty or two hundred end-to-end tests
that sleep five seconds, find buttons by English text, pass only on the
newest iPhone simulator at default text size, and fail one run in five.
Nobody tried the app offline, with the camera permission denied, from a
link while it was closed, or after updating over the previous version. This
skill gives the shape of a suite that catches what breaks on phones, the
tools per stack, and the conditions to test. Stack detail in
`mobile-react-native`, `mobile-flutter`, `mobile-ios`, `mobile-android`.

## 1. Shape

- Many fast tests on logic: view models and notifiers (loading, data,
  empty, error, retry), the sync engine (outbox order, idempotency keys
  reused on retry, conflicts), URL to route parsing, formatting of dates,
  money and plurals, local migrations.
- Component or widget tests for each screen in each state, with fake
  repositories rather than a network.
- A few end-to-end journeys (5 to 15): sign-in, the main task, purchase or
  checkout, onboarding, a deep link. Run on every merge or nightly; unit
  and component tests on every pull request, in minutes.
- Snapshot or golden tests for key screens and components.

## 2. Tools by stack

| Stack | Unit | Component | End-to-end |
|---|---|---|---|
| React Native | Jest with `jest-expo` | React Native Testing Library | Maestro, Detox |
| Flutter | `flutter test`, mocktail | Widget tests, goldens | `integration_test`, Patrol, Maestro |
| iOS | Swift Testing, XCTest | Snapshot tests, previews | XCUITest, Maestro |
| Android | JUnit, coroutines-test, Turbine | Compose UI tests (Robolectric or device), Roborazzi, Paparazzi | Compose and Espresso instrumented tests, UI Automator, Maestro |

- Maestro is the default for journeys: YAML flows, black box, one flow for
  both platforms, waits built in, works with every stack.
- Detox for React Native teams that want grey-box sync with the app's
  idle state; XCUITest and Espresso when the team lives in Xcode or
  Android Studio. UI Automator or Patrol when a flow crosses system UI.

## 3. Stable end-to-end tests

- Find elements by id, not by text or position: `accessibilityIdentifier`
  (iOS), `testTag` with `testTagsAsResourceId = true` (Compose, so
  UI Automator and Maestro see it), `testID` (React Native),
  `Semantics(identifier:)` or a `Key` (Flutter). Ids look like
  `checkout.place-order`.
- Wait for conditions, never fixed sleeps: `waitForExistence(timeout:)`,
  Maestro's automatic waits and `extendedWaitUntil`, Detox
  `waitFor(...).withTimeout(...)`, Espresso idling resources,
  `composeTestRule.waitUntil`.
- Deterministic data: a seeded test backend reset per run, or a mock server
  selected by a launch argument; a fixed clock, locale and time zone where
  the UI shows them. Credentials from the environment, never in the flow.
- A clean app per test: Maestro `clearState`, Detox `launchApp({ delete:
  true })`, Android Test Orchestrator with `clearPackageData`.
- Animations off on Android test devices (`adb shell settings put global
  animator_duration_scale 0`, and the window and transition scales).
- A flaky test is a bug: fix the wait or the data, retry at most once in
  CI, and track the flake rate.

```yaml
# .maestro/order-status.yaml; run: maestro test -e PASSWORD=$DEMO_PASSWORD .maestro/
appId: com.acme.app
---
- launchApp:
    clearState: true
    permissions: { notifications: deny }
- tapOn: { id: "signin.email" }
- inputText: "demo@example.com"
- tapOn: { id: "signin.password" }
- inputText: ${PASSWORD}
- tapOn: { id: "signin.submit" }
- extendedWaitUntil:
    visible: "Orders"
    timeout: 10000
- tapOn: "Order 1042"
- assertVisible: "Paid"
```

## 4. The device matrix

| Dimension | Test on |
|---|---|
| Smallest screen | iPhone SE (375 by 667pt); a 360dp-wide Android phone |
| Large phone | The largest current iPhone and a large Android |
| Tablet or foldable | iPad and a 600dp+ Android window, if the app supports them |
| Text size | The largest accessibility size (iOS AX5, Android 200%) |
| Appearance | Dark mode, Increase Contrast, Reduce Motion |
| Language | A right-to-left locale (Arabic or Hebrew) and a long one (German) (`i18n`) |
| OS | The oldest supported version and the newest |
| Hardware | A low-end Android with 2 to 4 GB of RAM |

```bash
xcrun simctl ui booted content_size accessibility-extra-extra-extra-large
xcrun simctl ui booted appearance dark
adb shell settings put system font_scale 2.0
adb shell cmd uimode night yes
```

## 5. Conditions

- Offline and slow: airplane mode; Network Link Conditioner on iOS (a
  developer setting on devices, an Xcode additional tool for the
  simulator); `adb emu network speed gsm` and `adb emu network delay gprs`
  on the emulator; a proxy (Proxyman, Charles) to inject timeouts and 500s.
- Interruptions: an incoming call (`adb emu gsm call 5551234` on the
  emulator), a notification, the app switcher, background for several
  minutes and back.
- Process death on Android: Home, then `adb shell am kill com.acme.app`,
  then reopen from recents; or "Don't keep activities" in developer options.
- Low memory: Simulate Memory Warning in the simulator; background apps
  trimmed on Android.
- Configuration changes mid-screen: rotation, dark mode, font size, split
  screen. Input and scroll position survive.
- Background limits: `adb shell dumpsys deviceidle force-idle` puts the
  device in Doze to test scheduled work.

## 6. Flows that break

- **Permissions**: granted on first ask; denied; denied twice on Android
  (the dialog no longer appears); granted later in Settings; limited photo
  access. Reset with `xcrun simctl privacy booted reset all com.acme.app`,
  `adb shell pm revoke com.acme.app android.permission.CAMERA`, or
  `app.resetAuthorizationStatus(for: .camera)` in XCUITest.
- **Deep links**: cold (app not running) and warm; signed out (sign-in,
  then on to the target); an unknown id (a not-found screen); link
  verification.

```bash
xcrun simctl openurl booted "https://acme.com/orders/42"
adb shell am start -W -a android.intent.action.VIEW -d "https://acme.com/orders/42" com.acme.app
adb shell pm get-app-links com.acme.app
```

- **Push notifications**: received in the foreground, in the background,
  and tapped from a killed app (it opens the right screen); the path when
  notifications are denied. The simulator takes payload files:

```json
{ "Simulator Target Bundle": "com.acme.app",
  "aps": { "alert": { "title": "Order shipped", "body": "Order 1042 is on its way" } },
  "link": "acme://orders/1042" }
```

  `xcrun simctl push booted com.acme.app shipped.apns`; on Android, a test
  message from the Firebase console to the emulator's token; Detox has
  `device.sendUserNotification`.
- **Upgrades**: install the previous store build, create data, install the
  new build over it. Data migrates, the session holds, onboarding does not
  return.
- **Sessions**: an access token expiring mid-flow (refreshed silently), a
  refresh failing (signed out cleanly), sign-out then sign-in as another
  user (nothing of the first remains).
- **Purchases**: a StoreKit configuration file for local testing and
  sandbox accounts on iOS, Play License testing accounts; buy, cancel, restore.

## 7. Accessibility audits

- iOS: the Accessibility Inspector audit, `try app.performAccessibilityAudit()`
  in XCUITest, a VoiceOver pass on a device.
- Android: Accessibility Scanner, `AccessibilityChecks.enable()` in Espresso
  tests, a TalkBack pass.
- Flutter: `meetsGuideline(androidTapTargetGuideline)`,
  `labeledTapTargetGuideline`, `textContrastGuideline` in widget tests.
- React Native: queries by role and label fail when labels are missing;
  plus a screen reader pass on each platform.
- The checklist: every control has a label and role, focus follows reading
  order, targets reach 44pt or 48dp, contrast holds in both themes,
  nothing breaks at the largest text (`mobile-ux`).

## 8. Snapshot and golden tests

- Key screens and components in each state (empty, loading, error, long
  text, dark, largest text, right-to-left), with bundled fonts, fixed data
  and a fixed date.

```swift
@MainActor func testOrderCardAtLargestText() {
  let view = OrderCard(order: .sample).environment(\.dynamicTypeSize, .accessibility5)
  assertSnapshot(of: view, as: .image(layout: .device(config: .iPhoneSe)))
}
```

- Tools: swift-snapshot-testing on iOS; Paparazzi or Roborazzi (no device)
  on Android; `matchesGoldenFile` in Flutter (`flutter test
  --update-goldens` to re-record).
- Record and compare on one pinned environment (the same simulator, OS and
  CI image), or fonts differ by a pixel and every test fails.
- Review diffs as images in the pull request; re-record on purpose, never
  to make a red build green.

## 9. CI and device farms

- Pull requests: unit and component tests (Linux for Android, Flutter and
  JavaScript; macOS for iOS).
- Main or nightly: end-to-end flows on simulators and emulators (Gradle
  Managed Devices or the `reactivecircus/android-emulator-runner` action;
  `xcodebuild test` on a named simulator), or Maestro Cloud.
- The wider matrix on a device farm: Firebase Test Lab, BrowserStack, AWS
  Device Farm, Sauce Labs.

```bash
gcloud firebase test android run --type instrumentation \
  --app app-debug.apk --test app-debug-androidTest.apk \
  --device model=<model-id>,version=34   # gcloud firebase test android models list
```

- Keep the artefacts: JUnit reports (`maestro test --format junit`),
  `.xcresult` bundles, screenshots, videos, logcat.

## Check it

- The project's suites: `npx jest`, `flutter test`, `xcodebuild ... test`,
  `./gradlew testDebugUnitTest connectedDebugAndroidTest`, `maestro test
  .maestro/`.
- A new end-to-end flow run 10 times in a row without a failure:
  `for i in $(seq 10); do maestro test .maestro/order-status.yaml || break; done`.
- For each changed flow: offline, a denied permission, a cold deep link and
  an upgrade tried by hand if not automated; say which were not run.

## Avoid

Only end-to-end tests; fixed sleeps; elements found by translated text;
tests sharing state; real production accounts or passwords in flows; one
simulator at default text size as the whole matrix; skipping offline,
denied permissions, cold deep links and upgrades; snapshot baselines
recorded on a developer's machine; re-recording goldens to hide a change;
flaky tests left to retry forever.
