---
name: mobile-flutter
description: "Flutter apps: project structure by feature, state management with Riverpod or Bloc, navigation with go_router, theming from tokens with Material 3, composing widgets and const constructors, lists, async work, platform channels, choosing packages, testing (unit, widget, golden, integration), performance, and flavors. Read when the project uses Flutter."
---

# Flutter apps

The naive Flutter app: one 900-line screen with `setState` and HTTP calls
in its methods, a `FutureBuilder` whose future is created in `build` and
fires on every rebuild, `Navigator.push` everywhere and no deep links,
`Colors.blue` scattered through widgets, Material switches on an iPhone,
forty packages chosen by name, and no tests. This skill is how a Flutter
team structures, builds, tests and ships an app in 2026. Conventions in
`mobile-ux`, offline data in `mobile-data`, stores in `mobile-release`.

## 1. Structure

Feature first, with a small shared core; follow the project if it differs.

```
lib/main.dart               runApp(const ProviderScope(child: App()))
lib/app/                    router, theme, the App widget
lib/core/                   HTTP client, errors, storage, logging
lib/features/orders/data/          DTOs, API calls, repository, Drift tables
lib/features/orders/domain/        entities and interfaces, only when they earn it
lib/features/orders/application/   notifiers or blocs
lib/features/orders/presentation/  screens and widgets
test/ mirrors lib/; integration_test/ holds device journeys
```

Code generation with build_runner (`dart run build_runner watch -d`):
freezed for immutable models and unions, json_serializable,
riverpod_generator. Freezed 3 requires the `abstract` or `sealed` keyword:

```dart
@freezed
abstract class Order with _$Order {
  const factory Order({required String id, required int totalCents, required DateTime createdAt}) = _Order;
  factory Order.fromJson(Map<String, dynamic> json) => _$OrderFromJson(json);
}
```

## 2. State

Riverpod 3 with code generation by default; Bloc when the project uses it.

```dart
@riverpod
class Orders extends _$Orders {
  @override
  Future<List<Order>> build() => ref.watch(orderRepositoryProvider).fetchOrders();

  Future<void> cancel(String id) async {
    await ref.read(orderRepositoryProvider).cancel(id);
    if (!ref.mounted) return;
    ref.invalidateSelf();
  }
}

class OrdersScreen extends ConsumerWidget {
  const OrdersScreen({super.key});

  @override
  Widget build(BuildContext context, WidgetRef ref) {
    return switch (ref.watch(ordersProvider)) {
      AsyncData(:final value) when value.isEmpty => const EmptyOrders(),
      AsyncData(:final value) => OrderList(orders: value),
      AsyncError(:final error) => ErrorView(error, onRetry: () => ref.invalidate(ordersProvider)),
      _ => const OrdersSkeleton(),
    };
  }
}
```

- `ref.watch` in `build`, `ref.read` in callbacks, `ref.listen` for side
  effects (a snackbar, navigation). A refreshing provider still matches
  `AsyncData`, so data stays on screen during a refresh.
- Riverpod 3 retries failing providers automatically; after an `await`, a
  notifier checks `ref.mounted` before touching state.
- Bloc: sealed events and states, bloc_concurrency transformers
  (`droppable()` for submits, `restartable()` for search), `BlocSelector`.
- `setState` only for state local to one widget; no HTTP in widgets.

## 3. Navigation with go_router

```dart
GoRouter(
  initialLocation: '/orders',
  refreshListenable: auth, // a Listenable that fires on sign-in and sign-out
  redirect: (context, state) {
    final atSignIn = state.matchedLocation == '/sign-in';
    if (!auth.signedIn) return atSignIn ? null : '/sign-in';
    return atSignIn ? '/orders' : null;
  },
  routes: [
    GoRoute(path: '/sign-in', builder: (context, state) => const SignInScreen()),
    StatefulShellRoute.indexedStack(
      builder: (context, state, shell) => AppShell(shell: shell),
      branches: [
        StatefulShellBranch(routes: [
          GoRoute(path: '/orders', builder: (context, state) => const OrdersScreen(), routes: [
            GoRoute(path: ':id', builder: (context, state) => OrderScreen(id: state.pathParameters['id']!)),
          ]),
        ]),
        StatefulShellBranch(routes: [GoRoute(path: '/settings', builder: (context, state) => const SettingsScreen())]),
      ],
    ),
  ],
);
```

- `StatefulShellRoute.indexedStack` keeps a stack per tab. `context.go`
  sets the location (the stack follows the URL); `context.push` adds a
  screen on top. Typed routes with go_router_builder in larger apps.
- Auth lives in `redirect` with `refreshListenable`, not in every screen.
  Deep links arrive as locations once App Links and Universal Links are
  configured natively.

## 4. Theming

```dart
const seed = Color(0xFF0B6E4F);
MaterialApp.router(
  routerConfig: router,
  theme: ThemeData(colorScheme: ColorScheme.fromSeed(seedColor: seed), textTheme: text),
  darkTheme: ThemeData(
    colorScheme: ColorScheme.fromSeed(seedColor: seed, brightness: Brightness.dark), textTheme: text),
);
```

- Material 3 is the default. Widgets read `Theme.of(context).colorScheme`
  and `textTheme`, never literal colours or sizes; other tokens (success,
  warning, brand surfaces) go in a `ThemeExtension`.
- iOS feel where it matters: `Switch.adaptive`, `Slider.adaptive`,
  `CircularProgressIndicator.adaptive`, `showAdaptiveDialog` with
  `AlertDialog.adaptive`, `Icons.adaptive.share`. Branch on
  `Theme.of(context).platform` (testable), not `Platform.isIOS`.

## 5. Widgets

- Small widget classes, not methods returning widgets: a class is its own
  rebuild boundary and can be `const`. `const` wherever possible (the
  `prefer_const_constructors` lint finds them).
- `ValueKey(item.id)` on stateful items in lists that reorder or remove.
- Narrow rebuilds: `ref.watch(provider.select((s) => s.count))`, a
  `Consumer` around the part that changes, `MediaQuery.sizeOf(context)`
  rather than `MediaQuery.of(context).size`.
- Futures, streams and controllers are created in `initState` or a
  provider and disposed, never in `build`. After an `await`,
  `if (!context.mounted) return;` before using `context`.

## 6. Lists

- `ListView.builder`, with `itemExtent` or `prototypeItem` when rows share a
  height (cheaper layout, accurate jumps). Slivers for complex scroll
  views: `CustomScrollView`, `SliverAppBar`, `SliverList.builder`.
- Request the next page when the builder reaches `items.length - 5`; a
  footer for loading, error and the end. `RefreshIndicator.adaptive`.
- Never a long `Column` in a `SingleChildScrollView`; `shrinkWrap: true` on
  a long nested list builds every row.

## 7. Async work

- Timeouts and cancellation: Dio `BaseOptions(connectTimeout:
  Duration(seconds: 10), receiveTimeout: Duration(seconds: 20))`, a
  `CancelToken` cancelled in `ref.onDispose`, subscriptions cancelled in
  `dispose`.
- Heavy work off the UI isolate (16ms per frame at 60Hz, 8ms at 120Hz):
  `await Isolate.run(() => parseOrders(body))` for large JSON, images or
  crypto; `compute` in older code.
- Uncaught errors reach crash reporting through `FlutterError.onError` and
  `PlatformDispatcher.instance.onError` (`mobile-release`).

## 8. Platform integration

Pigeon generates typed platform channels instead of string method names:

```dart
// pigeons/device.dart, generated with: dart run pigeon --input pigeons/device.dart
@ConfigurePigeon(PigeonOptions(dartOut: 'lib/core/device.g.dart', swiftOut: 'ios/Runner/Device.g.swift',
    kotlinOut: 'android/app/src/main/kotlin/com/acme/app/Device.g.kt'))
@HostApi()
abstract class DeviceApi {
  @async
  int freeBytes();
}
```

- Plugins are federated: check the platforms a package supports first.
- permission_handler: `final s = await Permission.camera.request();` then
  proceed when `s.isGranted`, and offer `openAppSettings()` when
  `s.isPermanentlyDenied`. On iOS each permission must also be enabled in
  the `Podfile` (`PERMISSION_CAMERA=1` in `GCC_PREPROCESSOR_DEFINITIONS`);
  without it the status reads as permanently denied.

## 9. Choosing packages

- Fewer packages. Before adding one: verified publisher, recent releases,
  pub points, open issues, every platform you ship supported; the
  flutter.dev and dart.dev publishers first.
- Commit `pubspec.lock`; `flutter pub outdated`, then one major upgrade at
  a time with tests between.

## 10. Testing

```dart
testWidgets('shows orders after a retry', (tester) async {
  final repo = FakeOrderRepository(failFirst: true, orders: [sampleOrder]);
  await tester.pumpWidget(ProviderScope(
    overrides: [orderRepositoryProvider.overrideWith((ref) => repo)],
    child: const MaterialApp(home: OrdersScreen()),
  ));
  await tester.pumpAndSettle();
  await tester.tap(find.bySemanticsLabel('Retry'));
  await tester.pumpAndSettle();
  expect(find.text('Order 1042'), findsOneWidget);
});
```

- Unit tests for notifiers, blocs and repositories, with fakes or mocktail
  (`when(() => repo.fetchOrders()).thenAnswer((_) async => [sample])`);
  widget tests find what a user finds, text and semantics labels first.
- Goldens for key screens (`matchesGoldenFile('goldens/order_card.png')`,
  refreshed with `flutter test --update-goldens`): real fonts loaded in
  `test/flutter_test_config.dart`, generated on the same OS as CI.
- Accessibility matchers: `meetsGuideline(androidTapTargetGuideline)`,
  `iOSTapTargetGuideline`, `labeledTapTargetGuideline`,
  `textContrastGuideline`.
- `integration_test` for a few journeys on a device
  (`IntegrationTestWidgetsFlutterBinding.ensureInitialized()`); Patrol when
  a flow crosses system dialogs (`mobile-testing`).

## 11. Performance

- Profile mode on a real device (`flutter run --profile`), then DevTools:
  frame chart (UI and raster time), rebuild counts, CPU and memory. Impeller
  has mostly removed shader jank; slow frames are the app's own work.
- Costly: `Opacity` on a changing subtree (use `FadeTransition` or
  `AnimatedOpacity`), `ShaderMask`, `BackdropFilter`, anti-aliased clips on
  large areas. `RepaintBoundary` around parts that repaint often.
- Images decoded at the size shown: `Image.network(url, cacheWidth: 192)`,
  or `memCacheWidth` with cached_network_image. Little work before
  `runApp`; plugins initialised lazily.
- Size: `flutter build appbundle --analyze-size`; release builds with
  `--obfuscate --split-debug-info=build/symbols`, symbols uploaded for crash
  reports (`mobile-performance`).

## 12. Flavors and environments

```bash
flutter run --flavor dev --dart-define-from-file=env/dev.json
flutter build appbundle --flavor prod --dart-define-from-file=env/prod.json
flutter build ipa --flavor prod --dart-define-from-file=env/prod.json
```

- Values read with `const String.fromEnvironment('API_URL')` are compiled
  into the app: never secrets.
- Android `productFlavors` with `applicationIdSuffix`; iOS a scheme per
  flavor with matching configurations (`Debug-dev`, `Release-prod`).
- `version: 1.4.0+42` in `pubspec.yaml` is the version name and build
  number; CI overrides with `--build-name` and `--build-number`.

## Check it

- `dart format --output=none --set-exit-if-changed .`, `flutter analyze`,
  `flutter test`; `dart run build_runner build -d` leaves no diff.
- `flutter build apk --debug`, and on macOS `flutter build ios --no-codesign`.
- The changed flow on a simulator or emulator; integration tests on a
  device when the project has them.

## Avoid

Business logic and HTTP in widgets; futures created in `build`; `ref.read`
in `build`; `setState` for shared state; literal colours; methods returning
widgets; missing `const`; lists built eagerly; heavy parsing on the UI
isolate; performance judged in debug mode; secrets in `--dart-define`;
packages added without checking platforms and upkeep; goldens generated on
a different OS from CI.
