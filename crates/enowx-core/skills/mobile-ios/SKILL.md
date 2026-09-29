---
name: mobile-ios
description: "Native iOS with Swift and SwiftUI: state with Observation, navigation stacks, lists, concurrency with async/await, actors and MainActor, networking with URLSession, persistence with SwiftData or Core Data, the Keychain, app lifecycle and background tasks, permissions and privacy manifests, accessibility, testing, and build configurations. Read when the project is a native iOS app."
---

# Native iOS with Swift and SwiftUI

The naive iOS app: `NavigationView` and `ObservableObject` copied from old
tutorials, `DispatchQueue.main.async` sprinkled until the warnings stop, a
fetch in `onAppear` that reruns on every return and is never cancelled, the
token in `UserDefaults`, dates that fail to decode because the server sends
fractional seconds, no privacy manifest, and a crash on the first camera
request because the usage string is missing. This skill is current SwiftUI
practice for iOS 17 and later. Conventions in `mobile-ux`, sync in
`mobile-data`, signing and stores in `mobile-release`.

## 1. Targets and project

- SwiftUI first; UIKit through `UIViewRepresentable` and
  `UIViewControllerRepresentable` where SwiftUI falls short,
  `UIHostingController` to bring SwiftUI into a UIKit app.
- Deployment target iOS 17 for a new app (Observation, SwiftData,
  `ContentUnavailableView`); newer APIs behind `if #available(iOS 26, *)`.
- Swift 6 language mode with complete concurrency checking. Xcode 26
  projects default to main-actor isolation (Swift 6.2 approachable
  concurrency); keep the settings of a project you did not create.
- Features in local Swift packages (faster builds, working previews); new
  dependencies through Swift Package Manager, since CocoaPods trunk is
  scheduled to become read-only in December 2026.
- Environments as configurations and schemes, values in xcconfig files read
  through Info.plist (`APIBaseURL = $(API_BASE_URL)`):

```
// Config/Staging.xcconfig; "/$()/" stops "//" starting a comment
API_BASE_URL = https:/$()/api.staging.acme.com
PRODUCT_BUNDLE_IDENTIFIER = com.acme.app.staging
```

## 2. State with Observation

```swift
@MainActor @Observable
final class OrdersModel {
  private(set) var orders: [Order] = []
  private(set) var failure: Error?
  private let api: OrdersAPI
  init(api: OrdersAPI) { self.api = api }

  func load() async {
    do { orders = try await api.orders(); failure = nil }
    catch is CancellationError {}
    catch { failure = error }
  }
}

struct OrdersView: View {
  @State private var model: OrdersModel
  init(api: OrdersAPI) { _model = State(initialValue: OrdersModel(api: api)) }

  var body: some View {
    List(model.orders) { order in
      NavigationLink(value: Route.order(order.id)) { OrderRow(order: order) }
    }
    .task { await model.load() }       // cancelled when the view goes away
    .refreshable { await model.load() }
  }
}
```

- `@State` owns what the view creates; `@Bindable` for bindings into an
  observable passed in; `@Environment(Model.self)` for app-wide models.
  Observation tracks only the properties `body` reads.
- A model where there is loading, validation or logic to test; plain
  `@State` otherwise. `ObservableObject` in existing code: migrated a
  feature at a time. `@SceneStorage` for per-window UI state.

## 3. Navigation

```swift
enum Route: Hashable { case order(Order.ID), settings }
@State private var path: [Route] = []

NavigationStack(path: $path) {
  OrdersView(api: api)
    .navigationDestination(for: Route.self) { route in
      switch route { case .order(let id): OrderDetailView(id: id); case .settings: SettingsView() }
    }
}
.onOpenURL { url in if let route = Route(url: url) { path = [route] } }
```

- One `NavigationStack` per tab; `NavigationSplitView` on iPad. Sheets with
  `.presentationDetents([.medium, .large])` and
  `.interactiveDismissDisabled(hasChanges)`.
- Universal Links: Associated Domains (`applinks:acme.com`) plus
  `/.well-known/apple-app-site-association` on the server.

## 4. Lists and the cost of body

- `List` for standard lists (reuse, swipe actions, edit mode); `LazyVStack`
  in a `ScrollView` for custom ones. `ForEach` over stable ids: never
  indices of a mutable array, never `\.self` on values that repeat.
- `body` runs often: no formatters built in it (`Text(date, format:
  .dateTime.day().month())`), no sorting of large arrays, no I/O. No
  `AnyView`; an `if` changes identity, a modifier value keeps it.
- `AsyncImage` has no disk cache: feeds use Nuke or Kingfisher. Debug with
  `let _ = Self._printChanges()` in `body` (`mobile-performance`).

## 5. Concurrency

- `.task` and `.task(id:)` for work tied to a view (cancelled and restarted
  for you); `Task {}` inherits the actor; `Task.detached` almost never.
- `@MainActor` on UI models; actors for shared mutable state; `Sendable`
  across boundaries (`@unchecked Sendable` only with a lock and a comment).
- Under approachable concurrency, nonisolated async functions run on the
  caller's actor; mark heavy work `@concurrent` to take it off the main one.
- `async let` for a fixed set of parallel calls, `withThrowingTaskGroup` for
  a dynamic set; `try Task.checkCancellation()` in long loops;
  `CancellationError` is never shown to the user.
- One token refresh for all callers, despite actor reentrancy:

```swift
actor TokenProvider {
  private var token: Token?
  private var refreshing: Task<Token, Error>?

  func current() async throws -> Token {
    if let token, !token.expiresSoon { return token }
    if let refreshing { return try await refreshing.value }
    let task = Task { try await refreshToken() }
    refreshing = task
    defer { refreshing = nil }
    let fresh = try await task.value
    token = fresh
    return fresh
  }
}
```

## 6. Networking

- `let (data, response) = try await session.data(for: request)`, status
  checked, then decoded. One configured session:
  `timeoutIntervalForRequest = 20` (default 60), `waitsForConnectivity =
  true` for requests that may wait (bounded by `timeoutIntervalForResource`),
  `allowsConstrainedNetworkAccess = false` for optional heavy downloads.
- `.iso8601` rejects fractional seconds; accept both:

```swift
decoder.dateDecodingStrategy = .custom { decoder in
  let text = try decoder.singleValueContainer().decode(String.self)
  if let date = try? Date.ISO8601FormatStyle(includingFractionalSeconds: true).parse(text) { return date }
  return try Date.ISO8601FormatStyle().parse(text)
}
```

- Errors mapped in one place (`URLError`, HTTP status, decoding); retries
  only for idempotent requests (`mobile-data`). Large transfers on a
  background session continue while suspended. No `NSAllowsArbitraryLoads`.

## 7. Persistence and the Keychain

| Data | Where |
|---|---|
| Records used offline | SwiftData; Core Data or GRDB for heavy queries or older targets |
| Small preferences | `UserDefaults`: never secrets or large data |
| Files the app manages | Application Support; re-downloadable ones in Caches (purgeable) |
| Tokens, secrets | Keychain |

- SwiftData: `.modelContainer(for: Draft.self)`, `@Query(sort:
  \Draft.updatedAt, order: .reverse)`, `context.insert`, `try
  context.save()` at explicit points. Schema changes through
  `VersionedSchema` and a `SchemaMigrationPlan`, tested from every shipped
  version. With CloudKit: no unique attributes, every property optional or
  defaulted, relationships optional.

```swift
func saveRefreshToken(_ token: String) throws {
  let query: [String: Any] = [kSecClass as String: kSecClassGenericPassword,
                              kSecAttrService as String: "com.acme.app.auth",
                              kSecAttrAccount as String: "refresh"]
  SecItemDelete(query as CFDictionary)
  var item = query
  item[kSecValueData as String] = Data(token.utf8)
  item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
  let status = SecItemAdd(item as CFDictionary, nil)
  guard status == errSecSuccess else { throw KeychainError(status: status) }
}
```

- `AfterFirstUnlockThisDeviceOnly` when background work needs the token,
  `WhenUnlockedThisDeviceOnly` otherwise; `ThisDeviceOnly` keeps it out of
  backups. Keychain items survive deleting the app: on the first launch
  after install (no flag in `UserDefaults` yet), delete stale ones.

## 8. Lifecycle, background work and push

- `scenePhase`: `.background` saves and stops work, `.active` refreshes
  stale data; `.inactive` (the app switcher) is not background. A save that
  must finish runs inside `beginBackgroundTask` (about 30 seconds).
- BGTaskScheduler: app refresh (about 30 seconds, when the system chooses
  from usage) and processing tasks (minutes, usually idle and charging).
  Identifiers in `BGTaskSchedulerPermittedIdentifiers`, Background Modes on,
  `.backgroundTask(.appRefresh("com.acme.refresh")) { await sync() }`, the
  next request submitted each run. iOS 26 adds
  `BGContinuedProcessingTaskRequest` for user-started work with progress.
- Push: APNs with a token-based `.p8` key on the server;
  `requestAuthorization(options: [.alert, .badge, .sound])` at a sensible
  moment, or `.provisional`; the device token sent on every launch (it
  changes); taps routed through `UNUserNotificationCenterDelegate` to a deep
  link. Silent pushes are throttled: never the only way data arrives.

## 9. Permissions and privacy

- Every protected resource needs its Info.plist string
  (`NSCameraUsageDescription`, `NSLocationWhenInUseUsageDescription`,
  `NSMicrophoneUsageDescription`, `NSFaceIDUsageDescription`); without it
  the app crashes on the request.
- `PrivacyInfo.xcprivacy` for the app and each SDK: data collected,
  tracking domains, and a reason per required-reason API under
  `NSPrivacyAccessedAPITypes`: `CA92.1` (`UserDefaults`), `C617.1` (file
  timestamps), `35F9.1` (system boot time), `E174.1` (disk space). The
  Organizer builds the combined privacy report from an archive.
- App Tracking Transparency only when data is linked with other companies'
  apps or sites for ads or brokers, never for first-party analytics.

## 10. Accessibility

- Text styles for all text; `@ScaledMetric var icon = 24` beside text;
  `dynamicTypeSize.isAccessibilitySize` or `ViewThatFits` to stack rows at
  the largest sizes.
- VoiceOver: `.accessibilityLabel` on icon-only controls,
  `.accessibilityAddTraits(.isHeader)`, `.accessibilityElement(children:
  .combine)` for rows, `.accessibilityAction(named:)`, `Image(decorative:)`
  for decoration.
- `accessibilityReduceMotion` swaps movement for fades (`motion-stacks`);
  previews for empty, error, dark and `.dynamicTypeSize(.accessibility3)`.

## 11. Testing

```swift
import Testing
@testable import Acme

@MainActor struct OrdersModelTests {
  @Test func showsOrdersAfterLoad() async {
    let model = OrdersModel(api: StubOrdersAPI(orders: [.sample]))
    await model.load()
    #expect(model.orders.count == 1 && model.failure == nil)
  }
}
```

- Swift Testing for new unit tests (`#require`, `@Test(arguments:)`);
  XCTest for UI and performance tests; HTTP stubbed with `URLProtocol`.
- XCUITest for a few journeys: elements by `accessibilityIdentifier`, a
  scenario passed in `app.launchEnvironment`, `waitForExistence(timeout: 5)`,
  and `try app.performAccessibilityAudit()` on key screens.

## Check it

```bash
xcrun simctl list devices available
xcodebuild -scheme Acme -destination 'platform=iOS Simulator,name=iPhone 17' build test | xcbeautify
swift test --package-path Packages/Features
xcrun simctl openurl booted "acme://orders/42"
```

- Once with the Thread Sanitizer on; Simulate Memory Warning on heavy
  screens; no new concurrency warnings in the Swift 6 build.

## Avoid

`NavigationView`; new `ObservableObject` code in an Observation app; work in
`onAppear` without cancellation; `DispatchQueue.main.async` to silence the
checker; `Task.detached` by habit; formatters or sorting in `body`;
`AnyView`; tokens in `UserDefaults`; missing usage strings or privacy
manifest entries; ATT prompts without tracking; background work assumed to
run on time.
