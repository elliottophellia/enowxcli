---
name: mobile-release
description: "Shipping mobile apps: versions and build numbers, signing keys and certificates kept out of the repository, App Store Connect and TestFlight, Google Play tracks, review guidelines that commonly reject apps, privacy labels and data safety forms, staged rollouts, crash reporting, over-the-air updates and their limits, release notes, and CI for mobile. Read before preparing a build for testers or the stores."
---

# Shipping mobile apps

The naive release: version numbers edited by hand and reused, the keystore
committed next to the code, a build sent to review with a sign-in wall and
no demo account, privacy labels that say "no data collected" beside an
analytics SDK, 100% of users on day one with no crash reporting, crashes
that cannot be read because no symbols were uploaded, and an over-the-air
update that changes what the reviewed app does. This skill is the release
process a mobile team trusts. Stack details are in `mobile-ios`,
`mobile-android`, `mobile-react-native` and `mobile-flutter`.

## 1. Versions and build numbers

| Where | User-facing version | Build number |
|---|---|---|
| iOS | `CFBundleShortVersionString` (`MARKETING_VERSION`) | `CFBundleVersion` (`CURRENT_PROJECT_VERSION`), higher for every upload |
| Android | `versionName` | `versionCode`, an integer higher for every upload, at most 2100000000 |
| Flutter | `version: 1.4.0+42` in `pubspec.yaml` | the `+42` |
| Expo | `version` in the app config | `ios.buildNumber`, `android.versionCode`, or EAS remote versions with `autoIncrement` |

- The version means something to users (major, minor, patch); the build
  number comes from CI (a run number or a stored counter), never typed.
  One build number for both platforms makes support simpler.
- Tag the released commit (`v1.4.0`) and keep a changelog.
- Old versions stay installed for months: the backend accepts them, or the
  app checks a minimum supported version and shows "Update required" only
  for real incompatibility or security.

## 2. Signing, kept out of the repository

- iOS: a distribution certificate and an App Store provisioning profile.
  Xcode's automatic signing for development; in CI, an App Store Connect
  API key (`.p8`, key id, issuer id) with fastlane match (certificates
  encrypted in a private repository or bucket), EAS credentials or Xcode
  Cloud. Distribution certificates expire after a year.
- Android: an upload key you hold, and Play App Signing, where Google holds
  the key users' devices trust. A lost upload key is reset through Play
  Console support, not the end of the app.

```bash
keytool -genkeypair -v -keystore upload.jks -keyalg RSA -keysize 2048 -validity 10000 -alias upload
```

```kotlin
signingConfigs {
  create("release") {
    storeFile = file(System.getenv("UPLOAD_KEYSTORE_PATH") ?: "missing.jks")
    storePassword = System.getenv("UPLOAD_KEYSTORE_PASSWORD")
    keyAlias = System.getenv("UPLOAD_KEY_ALIAS")
    keyPassword = System.getenv("UPLOAD_KEY_PASSWORD")
  }
}
```

- Keystores, `.p8`, `.p12`, profiles and passwords live in the CI secret
  store or EAS, never in git; `.gitignore` covers `*.jks`, `*.keystore`,
  `*.p8`, `*.p12`, `*.mobileprovision`. Never print them in logs.

## 3. iOS: App Store Connect and TestFlight

- The app record: name and subtitle (30 characters each), keywords (100),
  description (4000), promotional text (170), support and privacy policy
  URLs, age rating, category.
- Uploads come from the Organizer, `eas submit`, or fastlane
  (`upload_to_testflight`), built with the Xcode and SDK Apple requires
  (raised each spring; Xcode 26 and the iOS 26 SDK since April 2026).
- `ITSAppUsesNonExemptEncryption` set to `NO` in Info.plist when the app
  only uses HTTPS and system encryption, so each build skips the question.
- TestFlight: up to 100 internal testers (team members, no review) and
  10,000 external ones (the first build of a version passes Beta App
  Review); builds expire after 90 days.
- Screenshots from the real app for the largest iPhone (6.9-inch) and iPad
  (13-inch) sizes; smaller sizes are scaled from them.

## 4. What App Review rejects most

| Guideline | What trips it | Do instead |
|---|---|---|
| 2.1 App Completeness | Crashes, placeholder text, dead links, a sign-in wall with no demo account | Test the release build; a working demo account and notes in App Review Information |
| 2.3 Accurate Metadata | Screenshots or text promising what the app does not do | Real screenshots and descriptions |
| 4.2 Minimum Functionality | A website in a wrapper, too little to do | Native value beyond the website |
| 3.1.1 In-App Purchase | Digital goods or subscriptions unlocked outside StoreKit | In-app purchase; physical goods and real-world services pay normally; regional exceptions (US storefront links since 2025) |
| 4.8 Login Services | Google or Facebook sign-in with no privacy-focused equivalent | Add Sign in with Apple |
| 5.1.1(v) Account deletion | Accounts can be created but not deleted in the app | A deletion flow inside the app |
| 5.1.1 Data collection | Vague purpose strings, personal data not needed, sign-in forced for features that do not need it | Specific purpose strings, less data |
| 4.3 Spam | Near-identical apps from one template | One app with configuration |

## 5. Privacy declarations

- App Store privacy labels: every data type collected, whether linked to
  the user, whether used for tracking, including what SDKs collect
  (analytics, crash reporting, ads, attribution). The privacy manifests
  (`mobile-ios`) and the Organizer's privacy report must agree with them.
- Google Play Data safety form: data collected and shared, purposes,
  encryption in transit, how users request deletion. Play also requires
  account deletion inside the app and through a web link declared in the
  console.
- A privacy policy URL on both stores and inside the app. Declarations
  describe what the build does; never guess them.

## 6. Android: Google Play

- Tracks: internal testing (up to 100 testers, available in minutes),
  closed testing (lists or Google Groups), open testing, production. New
  personal developer accounts must run a closed test with at least 12
  testers opted in for 14 continuous days before production.
- Android App Bundles (`./gradlew bundleRelease`), not APKs.
- Target API level: API 35 since August 31, 2025; API 36 for new apps and
  updates from August 31, 2026. Check the console's current rule.
- Native libraries must support 16 KB memory pages for apps targeting
  Android 15 and later (since November 1, 2025): recent React Native,
  Flutter and NDK r28 toolchains handle it; old prebuilt `.so` files fail.
- Sensitive permissions need a declaration and justification in the
  console: background location, SMS and call log, all-files access,
  `QUERY_ALL_PACKAGES`, exact alarms, foreground service types, broad photo
  and video access. Remove the ones libraries add but the app does not use
  (read the merged manifest).
- Listing: title 30 characters, short description 80, full description
  4000, icon 512 by 512, feature graphic 1024 by 500, 2 to 8 screenshots
  per device type, release notes up to 500 characters per language.

## 7. Staged rollouts

- Play: release to a small share (1 to 5%), then widen over days (20%, 50%,
  100%) while crash and ANR rates hold. Android vitals flags a
  user-perceived crash rate over 1.09% and an ANR rate over 0.47%. Halt the
  rollout when numbers rise; a release cannot be rolled back, only replaced
  by a higher `versionCode`.
- iOS phased release spreads automatic updates over 7 days (1, 2, 5, 10,
  20, 50, 100%) and can be paused; manual downloads get the new version at
  once. A bad release is replaced by a fix (ask for an expedited review for
  a critical bug).
- Feature flags and server-side kill switches turn off a broken feature
  without a release. Play's In-App Updates API prompts for flexible or
  immediate updates.

## 8. Crash and error reporting

- Firebase Crashlytics or Sentry from the first build, with release and
  environment tags, an opaque user id and no personal data in breadcrumbs.
- Symbols for every release, or stack traces are unreadable: iOS dSYMs
  (the Crashlytics run script or `sentry-cli debug-files upload`), Android
  R8 `mapping.txt` (uploaded by the Crashlytics or Sentry Gradle plugin,
  also to Play for vitals), Flutter `--split-debug-info` symbols, React
  Native source maps per platform and release (Hermes included).
- Flutter wiring:

```dart
FlutterError.onError = FirebaseCrashlytics.instance.recordFlutterFatalError;
PlatformDispatcher.instance.onError = (error, stack) {
  FirebaseCrashlytics.instance.recordError(error, stack, fatal: true);
  return true;
};
```

- Aim for 99.5% or more crash-free users, alert on a regression per
  release, and read new issues daily during a rollout.

## 9. Over-the-air updates

- EAS Update (Expo) ships JavaScript and assets; Shorebird ships Dart code
  for Flutter. CodePush ended with App Center's retirement in March 2025.
- Only code that runs in the interpreter or VM changes: never native code,
  permissions, entitlements, SDK versions or config plugins.
- Store rules: Apple accepts interpreted code that does not change the
  app's primary purpose, create a storefront or bypass review and security;
  Google Play forbids downloading executable code outside Play except code
  run in a VM or interpreter. Use OTA for fixes, not features that would
  need review.
- Runtime versions (the `fingerprint` policy on EAS, the matching release
  on Shorebird) keep an update away from binaries it cannot run on;
  channels per environment; a test on the preview channel first; a staged
  rollout; a rollback plan (republish the previous update, or roll back in
  the Shorebird console).

```bash
eas update --channel production --message "Fix totals rounding"
shorebird release android && shorebird patch android
```

## 10. Release notes and store assets

- Notes say what changed for users, in their words, the fix they asked for
  first (`writing`). Localised with the app (`i18n`).
- Screenshots from the real app with a clean status bar (`xcrun simctl
  status_bar booted override --time 9:41 --batteryLevel 100`), realistic
  sample content, no invented ratings, awards or testimonials. fastlane
  snapshot and screengrab, or Maestro, automate them.

## 11. CI for mobile

| Tool | Fits |
|---|---|
| EAS Build, Submit and Workflows | Expo apps, managed credentials |
| fastlane | Any native project: match, gym, pilot, deliver, supply |
| Xcode Cloud | iOS only, Apple-managed signing |
| Codemagic, Bitrise | Mobile-focused CI for every stack |
| GitHub Actions with macOS runners | Your own pipelines; macOS minutes cost 10 times Linux ones |

- Pull requests: lint, unit tests, debug builds of both platforms. Main: a
  signed build to TestFlight and the internal track. A tag: the release
  candidate, submitted by a person.
- Cache Gradle, Swift packages, CocoaPods, `node_modules` and the pub cache.

```ruby
lane :beta do
  app_store_connect_api_key(key_id: ENV["ASC_KEY_ID"], issuer_id: ENV["ASC_ISSUER_ID"], key_content: ENV["ASC_KEY_P8"])
  match(type: "appstore", readonly: true)
  increment_build_number(build_number: ENV["GITHUB_RUN_NUMBER"])
  build_app(scheme: "Acme", export_method: "app-store")
  upload_to_testflight(skip_waiting_for_build_processing: true)
end
```

## 12. What the agent does and does not do

Prepare builds, metadata, release notes, privacy answers drafted from the
code and SDK list, and CI configuration. Do not submit to a store, promote
a track, change a rollout, bump the release version, or create or rotate
signing keys unless the user says so. Report what could not be verified
(an upload, a review, a device run).

## Check it

- `git ls-files | grep -E '\.(jks|keystore|p8|p12|mobileprovision)$'`
  prints nothing; CI reads signing material from secrets.
- The release build installed on a device: the flows walked, the version
  and build number shown in About match the release.
- A test crash from a release build appears symbolicated in the dashboard.
- The privacy labels and Data safety form match the SDK list; the demo
  account signs in; the privacy policy URL loads.
- Android native libraries checked for 16 KB alignment (the APK Analyzer in
  Android Studio, or `zipalign -c -P 16 -v 4 app.apk`).

## Avoid

Hand-edited or reused build numbers; signing keys in git or logs; review
builds without a demo account; privacy declarations that ignore SDKs;
100% rollouts with no crash reporting; releases without symbols; OTA
updates that change native code or the app's purpose; store screenshots
that are not the app; submitting or promoting builds nobody asked for.
