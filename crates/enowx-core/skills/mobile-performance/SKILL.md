---
name: mobile-performance
description: "Mobile performance: cold start time, smooth scrolling at 60 or 120 frames a second, list virtualisation, image sizes, memory and leaks, battery (background work, location, wake locks), network batching, app size, and profiling with Instruments, Android Studio, React Native and Flutter DevTools. Read before optimising a mobile app or building a heavy screen."
---

# Mobile performance

The naive approach: optimising by guesswork in a debug build on the newest
flagship; a launch that runs migrations, fetches and five SDK
initialisations before the first frame; 4000px photos decoded into 48 MB
bitmaps for 64pt thumbnails; a list that renders every row; location at
best accuracy all day; three analytics SDKs in a 150 MB download. This skill
gives the targets, the causes that matter, and the tools that show them.
Stack detail in `mobile-ios`, `mobile-android`, `mobile-react-native`,
`mobile-flutter`; native animation in `motion-stacks`.

## 1. Targets

| Metric | Target | Measured with |
|---|---|---|
| Cold start to useful content | Under about 2s on a mid-range device; Apple aims for the first frame in 400ms; Android vitals calls cold starts of 5s, warm 2s and hot 1.5s excessive | Xcode Organizer, MetricKit, App Launch instrument; Android vitals, Macrobenchmark |
| Frames | 16.7ms per frame at 60Hz, 8.3ms at 120Hz, no drops while scrolling | Animation Hitches instrument; JankStats, `FrameTimingMetric` |
| Main thread stalls | None over 250ms (a hang on iOS); Android shows an ANR after 5s without input response | Hangs instrument, Organizer; Android vitals (ANR rate over 0.47% is bad) |
| Memory | Stable after repeating a flow; within budget on the oldest supported phones | Allocations, Leaks, memory graph; Android Studio Memory, LeakCanary |
| Download size | As small as the features allow; over 200 MB, iOS asks before a cellular download | App Store Connect size report; APK Analyzer, `bundletool` |

Measure on a three- to five-year-old mid-range Android and the oldest
supported iPhone, never only on the development machine's simulator.

## 2. Measure first

- Release builds (Flutter profile mode): debug builds are several times
  slower and misleading. Instruments profiles a Release build by default.
- One change at a time; before and after on the same device; the median of
  at least 5 runs.
- Field data beats the lab: Xcode Organizer and MetricKit on iOS, Android
  vitals on Play, and the performance features of Crashlytics or Sentry.

## 3. Startup

- The path: process start, app initialisation, first frame, first useful
  content. Shorten each part; the user sees the sum.
- Defer what the first screen does not need: analytics, ads, remote config,
  feature SDKs, big dependency graphs. Android: App Startup to control
  library initialisers; nothing heavy in `Application.onCreate`. iOS:
  nothing heavy in the `App` initialiser or `didFinishLaunching`; fewer
  dynamic frameworks (Swift packages link statically by default).
- No synchronous disk or network on the main thread at launch (StrictMode
  in Android debug builds reports it). Open the database off the main
  thread; show cached content first, refresh after.
- The splash only until the first frame is ready (`installSplashScreen()`
  with a short `setKeepOnScreenCondition` on Android); a skeleton, not a
  longer splash, while content loads.
- Android: baseline profiles precompile startup code; `reportFullyDrawn()`
  (or `ReportDrawnWhen` in Compose) marks when content is really there.
- React Native: Hermes bytecode, lazy imports for heavy screens, no large
  JSON imported at startup. Flutter: little before `runApp`, plugins
  initialised lazily.

## 4. Rendering and scrolling

- Every long list virtualised: `List` or `LazyVStack`, `LazyColumn`,
  FlashList, `ListView.builder`; fixed row heights where possible.
- The main thread does UI only. Parsing large responses, image work,
  crypto and sorting big collections go elsewhere: actors or `@concurrent`
  functions in Swift, `Dispatchers.Default` in Kotlin, `Isolate.run` in
  Dart, native code or the server in React Native.
- Shallow hierarchies; few stacked translucent layers (Android's "Debug GPU
  overdraw" shows them); blur, masks and shadows used sparingly in rows.
- Animations on the render thread: Core Animation and SwiftUI animations,
  Compose `graphicsLayer`, Reanimated, Flutter's animation widgets. Never
  layout changes computed per frame on the main thread.

## 5. Images and memory

- Memory is width times height times 4 bytes: a 12 MP photo (4032 by 3024)
  takes about 48 MB decoded, whatever its file size. Decode at the size
  shown times the screen scale, and ask the server or CDN for that size.
- Coil and Glide size to the view; Nuke and Kingfisher have resize
  processors; Flutter has `cacheWidth` and `ResizeImage`; expo-image
  downscales. Custom iOS code downsamples with ImageIO:

```swift
func downsample(_ url: URL, to size: CGSize, scale: CGFloat) -> UIImage? {
  let sourceOptions = [kCGImageSourceShouldCache: false] as CFDictionary
  guard let source = CGImageSourceCreateWithURL(url as CFURL, sourceOptions) else { return nil }
  let options = [kCGImageSourceCreateThumbnailFromImageAlways: true,
                 kCGImageSourceShouldCacheImmediately: true,
                 kCGImageSourceCreateThumbnailWithTransform: true,
                 kCGImageSourceThumbnailMaxPixelSize: max(size.width, size.height) * scale] as CFDictionary
  guard let image = CGImageSourceCreateThumbnailAtIndex(source, 0, options) else { return nil }
  return UIImage(cgImage: image)
}
```

- WebP or AVIF for photos, vectors (SF Symbols, VectorDrawable) for icons.
- Bounded caches, emptied on memory warnings
  (`UIApplication.didReceiveMemoryWarningNotification`) and when the UI is
  hidden (`onTrimMemory(TRIM_MEMORY_UI_HIDDEN)`).
- Leaks: closures capturing `self` strongly, timers and observers never
  removed (iOS); Activities or Contexts held by singletons, listeners not
  unregistered (Android); controllers not disposed (Flutter); effects
  without cleanup (React Native). Find them with Instruments Leaks and the
  memory graph debugger, LeakCanary in debug builds, DevTools memory
  snapshots.

## 6. Battery

- Batch network work: one sync per foreground rather than a request per
  screen; no polling loops (push, or long intervals). Each radio wake-up
  costs more than the bytes it carries.
- Background work only through the OS schedulers (BGTaskScheduler,
  WorkManager) with constraints: charging and unmetered for heavy jobs.
- Location: the lowest accuracy that works (`kCLLocationAccuracyHundredMeters`,
  `Priority.PRIORITY_BALANCED_POWER_ACCURACY`), significant-change updates
  or geofences instead of continuous tracking, stopped as soon as the
  feature is done.
- No wake locks without a timeout; keep the screen on only on the screen
  that needs it (a player, turn-by-turn), through the window flag.
- Timers, sensors and animations stop when their screen is not visible.
- Honour Low Power Mode and Battery Saver
  (`ProcessInfo.processInfo.isLowPowerModeEnabled`,
  `PowerManager.isPowerSaveMode`): skip prefetching and decorative motion.
- Watch the energy gauge in Xcode, Android Studio's Power Profiler, and
  Android vitals' excessive wake lock and wakeup reports.

## 7. Network

- One shared client (one `URLSession`, one `OkHttpClient`, one Dio), so
  connections are reused over HTTP/2.
- Compressed responses (the clients decode gzip for you), only the fields a
  screen needs, one request per screen where the API allows, ETags for
  repeat loads.
- Prefetch only likely next content (the next page, the next screen's
  images), and only on unmetered networks with battery to spare.
- Timeouts, retries and cancellation as in `mobile-data`.

## 8. App size

- Measure: the App Thinning Size Report (export with thinning enabled) or
  App Store Connect's size page; APK Analyzer and `bundletool get-size
  total`; `flutter build appbundle --analyze-size`; Expo Atlas for the
  JavaScript bundle.
- Remove before compressing: one analytics SDK, not three; libraries used
  for one function; unused fonts and weights; debug-only tools in release.
- R8 with resource shrinking; App Bundles split by ABI, density and
  language; images as WebP or AVIF, icons as vectors; fonts subset.
- Large optional content downloaded after install: Play Asset Delivery,
  Apple's Background Assets, or your own resumable downloads
  (`mobile-data`). Google Play caps the base module's compressed download
  at 200 MB.

## 9. Framework specifics

| Stack | Look for | Tools |
|---|---|---|
| React Native | JS frame rate dropping, re-renders, rows not memoised, animation on the JS thread, megabytes crossing into native code | React Native DevTools profiler, Perf Monitor, native profilers |
| Flutter | UI or raster thread over budget, large subtrees rebuilding (missing `const`, broad `watch`), `saveLayer` from `Opacity`, clips and shader masks, images not resized | DevTools performance and rebuild stats in profile mode |
| SwiftUI | Expensive `body`, broad invalidation, identity churn (`.id(UUID())`, `AnyView`), eager stacks, `GeometryReader` in rows | Instruments SwiftUI template, Time Profiler, Hangs |
| Compose | High recomposition counts, unstable parameters, state read in composition instead of layout or draw, lists without keys, no baseline profile | Layout Inspector, Perfetto system traces, Macrobenchmark, Compose compiler reports |

## 10. Profiling tools

- Instruments: Time Profiler (where CPU time goes), Hangs, Animation
  Hitches, Allocations, Leaks, App Launch, Network; the Xcode Organizer for
  launch, hang, memory, battery and disk-write metrics from the field.
- Android Studio: CPU with system traces, Memory with heap dumps,
  Power; Perfetto (ui.perfetto.dev) for whole-system traces; Macrobenchmark
  for startup and frame timing in CI.
- `adb shell dumpsys gfxinfo <package>` for frame statistics and
  `adb shell dumpsys meminfo <package>` for memory.
- React Native DevTools and Flutter DevTools as in their skills.

## Check it

```bash
adb shell am force-stop com.acme.app
adb shell am start -W -n com.acme.app/.MainActivity   # TotalTime is the cold start
adb shell dumpsys gfxinfo com.acme.app reset           # then scroll the list
adb shell dumpsys gfxinfo com.acme.app | grep -E "Janky frames|99th percentile"
flutter run --profile
```

- Numbers before and after for the metric changed, on the same mid-range
  device, release or profile build, median of several runs.
- Repeat a heavy flow 10 times: memory returns to its starting level;
  LeakCanary stays silent.
- The size report before and after adding a dependency.
- Say which device and build the numbers came from, and what was not
  measured.

## Avoid

Measuring debug builds or only flagships; work on the main thread at
launch; SDKs initialised before the first frame; lists that render every
row; full-size images for thumbnails; unbounded caches; listeners never
removed; polling; precise location all day; wake locks without timeouts;
prefetching on metered networks; dependencies added without a size check;
optimisations shipped without numbers.
