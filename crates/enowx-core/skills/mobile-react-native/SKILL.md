---
name: mobile-react-native
description: "React Native with Expo: project setup, Expo Router or React Navigation, styling, lists with FlashList, images, data and state, secure storage, animations with Reanimated and Gesture Handler, native modules, the New Architecture, performance on the JS thread, testing, and EAS builds and updates. Read when the project uses React Native or Expo."
---

# React Native with Expo

The naive React Native app: native folders edited by hand and lost at the
next prebuild, a `ScrollView` mapping 500 rows, animations driven by
`setState` that stutter while data loads, a `useEffect` fetch in every
screen, the token in AsyncStorage, performance judged in a debug build, and
an over-the-air update carrying JavaScript that needs a native module the
installed binary lacks. This is the current Expo way to build, measure and
ship. Conventions in `mobile-ux`, sync in `mobile-data`, stores in
`mobile-release`.

## 1. Project setup

- Expo by default (the React Native docs recommend a framework):
  `npx create-expo-app@latest`. A development build (`expo-dev-client`) as
  soon as any library has native code; Expo Go only for quick prototypes.
- Continuous Native Generation: `ios/` and `android/` come from
  `npx expo prebuild` and are ignored by git; native settings come from the
  app config and config plugins. A project that commits its native folders
  owns them: edit them there, never `prebuild --clean` over them.
- The New Architecture (Fabric, TurboModules, bridgeless) is the default
  since React Native 0.76 and the only one from 0.82; Expo SDK 54 was the
  last SDK with the legacy one. Check libraries on reactnative.directory.
- `npx expo install <pkg>` picks the version matching the SDK;
  `npx expo install --check` and `npx expo-doctor` find mismatches.

```ts
// app.config.ts
import type { ConfigContext, ExpoConfig } from "expo/config";

const variant = process.env.APP_VARIANT ?? "production";
const id = variant === "production" ? "com.acme.app" : `com.acme.app.${variant}`;

export default ({ config }: ConfigContext): ExpoConfig => ({
  ...config,
  name: variant === "production" ? "Acme" : `Acme (${variant})`,
  slug: "acme",
  scheme: "acme",
  userInterfaceStyle: "automatic", // without it the app stays light
  ios: { bundleIdentifier: id },
  android: { package: id },
  runtimeVersion: { policy: "fingerprint" },
  plugins: ["expo-router", ["expo-camera", { cameraPermission: "Allow Acme to scan receipts." }]],
  experiments: { typedRoutes: true },
});
```

- `EXPO_PUBLIC_*` values are inlined into the bundle: public. Read them
  statically (`process.env.EXPO_PUBLIC_API_URL`, never `process.env[name]`).
  Secrets stay on the server; build values live in EAS environment
  variables (`eas env:create`, per environment).
- `eas.json` profiles: `development` (`developmentClient`, internal
  distribution), `preview` (internal, own channel), `production` (channel
  `production`, `autoIncrement`); `appVersionSource: "remote"` under `cli`.

## 2. Navigation

Expo Router: files are routes, each deep-linkable, typed with
`experiments.typedRoutes`. React Navigation (native stack, bottom tabs)
where the project already uses it; Expo Router is built on it.

```
app/_layout.tsx                root Stack, providers, the auth guard
app/sign-in.tsx
app/compose.tsx                presentation: "modal"
app/(tabs)/_layout.tsx         <Tabs> with 3 to 5 screens
app/(tabs)/orders/_layout.tsx  a Stack inside the tab
app/(tabs)/orders/[id].tsx     /orders/42 and acme://orders/42
app/+not-found.tsx
```

```tsx
export default function RootLayout() {
  const { session } = useSession();
  return (
    <Stack>
      <Stack.Protected guard={!!session}>
        <Stack.Screen name="(tabs)" options={{ headerShown: false }} />
        <Stack.Screen name="compose" options={{ presentation: "modal" }} />
      </Stack.Protected>
      <Stack.Protected guard={!session}>
        <Stack.Screen name="sign-in" options={{ headerShown: false }} />
      </Stack.Protected>
    </Stack>
  );
}
```

- ``<Link href={`/orders/${id}`}>`` or `router.push(...)`; params with
  `useLocalSearchParams<{ id: string }>()`; `router.replace` after sign-in
  and sign-out, so back skips the form. Sheets: `presentation: "formSheet"`
  with `sheetAllowedDetents`. Native headers and tabs, not JavaScript ones.

## 3. Styling

- `StyleSheet.create` with tokens from one module, or the project's library
  (NativeWind, Unistyles, Tamagui); `Platform.select` for real differences.
- `useWindowDimensions()`, never `Dimensions.get` at module scope; safe
  areas from react-native-safe-area-context (the built-in `SafeAreaView` is
  deprecated). `Pressable` with `android_ripple`, `hitSlop` to reach 48dp.

## 4. Lists

```tsx
<FlashList
  data={orders}
  renderItem={renderOrder} // stable, defined outside the render or memoised
  keyExtractor={(o) => o.id}
  getItemType={(o) => o.kind}
  onEndReached={fetchNextPage}
  onEndReachedThreshold={0.5}
/>
```

- FlashList v2 (New Architecture) measures rows itself; on v1, set
  `estimatedItemSize` to the typical row height.
- Memoised rows with stable callbacks. FlashList recycles rows: local state
  leaks from one item to the next unless reset when the item changes.
- `FlatList` where FlashList is absent (`getItemLayout` for fixed heights).
  Never a `ScrollView` with `.map` for more than a screenful.

## 5. Images

```tsx
<Image source={{ uri: p.imageUrl }} placeholder={{ blurhash: p.blurhash }}
  contentFit="cover" transition={150} cachePolicy="memory-disk"
  recyclingKey={p.id} style={{ width: 64, height: 64 }} accessibilityLabel={p.name} />
```

expo-image, with the size shown requested from the server or CDN (a 64pt
thumbnail needs 192px at 3x, not the 4000px original); `Image.prefetch`
for the next screen; `recyclingKey` stops a recycled row flashing the
previous image.

## 6. Data, state and storage

Server data through TanStack Query, never copied into a global store. Tell
it about focus and connectivity once:

```ts
onlineManager.setEventListener((setOnline) =>
  NetInfo.addEventListener((state) => setOnline(!!state.isConnected)),
);
AppState.addEventListener("change", (s) => focusManager.setFocused(s === "active"));
```

Zustand for small client state, read through selectors (`useCart((s) => s.count)`).

| Store | For |
|---|---|
| expo-secure-store | Tokens and small secrets (Keychain, Keystore); values up to 2048 bytes |
| react-native-mmkv | Fast synchronous key-value: settings, flags, a query cache |
| expo-sqlite (with Drizzle) or op-sqlite | Relational and offline data (`mobile-data`) |
| AsyncStorage | Legacy projects only; unencrypted and slow |

```ts
await SecureStore.setItemAsync("refreshToken", token, {
  keychainAccessible: SecureStore.AFTER_FIRST_UNLOCK_THIS_DEVICE_ONLY,
});
await SecureStore.deleteItemAsync("refreshToken"); // on sign-out
```

## 7. Animation and gestures

Reanimated runs worklets on the UI thread, so a busy JavaScript thread drops
no frames; Gesture Handler (inside `GestureHandlerRootView`) is native.

```tsx
const x = useSharedValue(0);
const style = useAnimatedStyle(() => ({ transform: [{ translateX: x.value }] }));
const pan = Gesture.Pan()
  .onChange((e) => { x.value += e.changeX; })
  .onFinalize(() => { x.value = withSpring(0); });
// <GestureDetector gesture={pan}><Animated.View style={[styles.card, style]} /></GestureDetector>
```

- Reanimated 4 needs the New Architecture and adds CSS-like transitions;
  its worklets live in `react-native-worklets`. Legacy apps stay on 3.
- Layout animations (`entering={FadeIn.duration(200)}`, `LinearTransition`);
  reduced motion with `ReduceMotion.System` (`motion-stacks`). Never
  `setState` per frame or `Animated` with `useNativeDriver: false`.

## 8. Native code

- An existing library first (Expo SDK, then maintained community modules).
  Your own through the Expo Modules API: `npx create-expo-module@latest
  --local` creates `modules/<name>` with Swift and Kotlin sides.

```swift
import ExpoModulesCore

public class DeviceStorageModule: Module {
  public func definition() -> ModuleDefinition {
    Name("DeviceStorage")
    AsyncFunction("freeBytes") { () -> Int64 in
      let v = try URL(fileURLWithPath: NSHomeDirectory())
        .resourceValues(forKeys: [.volumeAvailableCapacityForImportantUsageKey])
      return v.volumeAvailableCapacityForImportantUsage ?? 0
    }
  }
}
```

  JavaScript loads it with `requireNativeModule("DeviceStorage")` from
  `expo-modules-core`. Disk space is a required-reason API: declare it in
  the privacy manifest (`mobile-ios`).
- Native configuration (Info.plist keys, manifest entries, Gradle
  settings) through a config plugin (`withInfoPlist`, `withAndroidManifest`
  from `expo/config-plugins`), not a hand-edited folder.

## 9. Performance

- Measure release builds only (`npx expo run:ios --configuration Release`,
  `npx expo run:android --variant release`); debug builds are several times
  slower. Hermes compiles bytecode at build time.
- Re-renders: with the React Compiler on (`experiments.reactCompiler`),
  manual memoisation is mostly unnecessary; without it, `memo` on rows and
  heavy children, `useCallback` for their callbacks, narrow selectors, and
  contexts split by how often they change.
- The JavaScript thread runs UI logic only: large JSON, crypto and image
  work move to native code or the server.
- Startup: keep the splash (`SplashScreen.preventAutoHideAsync()`) until
  fonts and cached data are ready, never until a network call returns.
- Tools: React Native DevTools (`j` in the Metro terminal) with the React
  profiler; the Perf Monitor (JS and UI frame rates); Expo Atlas for the
  bundle; Instruments and Android Studio natively (`mobile-performance`).

## 10. Testing

```tsx
test("adds an item to the cart", async () => {
  const user = userEvent.setup();
  render(<ProductScreen product={sample} />);
  await user.press(screen.getByRole("button", { name: "Add to cart" }));
  expect(await screen.findByText("1 item")).toBeOnTheScreen();
});
```

- Jest with `jest-expo` and React Native Testing Library; queries by role
  and text; the network mocked at the fetch layer (MSW), not in hooks.
- End-to-end with Maestro (YAML, any build) or Detox (grey-box, waits for
  the app to idle); `mobile-testing`.

## 11. Builds and updates

```bash
eas build --profile development --platform ios
eas build --profile production --platform all
eas submit --platform ios --latest      # only when asked
eas update --channel production --message "Fix rounding in cart totals"
```

- EAS Update ships JavaScript and assets only. A new native library, a
  permission, an SDK upgrade or a config plugin change needs a new binary.
  The `fingerprint` runtime version changes whenever native code does, so
  an update never reaches a binary it cannot run on.
- Updates download on launch and apply on the next one; roll back by
  republishing the previous update (`eas update:republish`); `mobile-release`.

## Check it

- `npx expo-doctor`, `npx tsc --noEmit`, `npx expo lint`, `npx jest`.
- `npx expo export --platform all` bundles both platforms without a device.
- A release build with the changed flow walked, the Perf Monitor steady
  while scrolling; Maestro flows if present.

## Avoid

Hand edits to generated native folders; Expo Go for an app with native
libraries; secrets in `EXPO_PUBLIC_` variables; `ScrollView` lists; row
renderers without memoisation; server data in Redux or Zustand; tokens in
AsyncStorage; JavaScript-driven animation; performance judged in debug; an
over-the-air update carrying a native change; libraries without New
Architecture support.
