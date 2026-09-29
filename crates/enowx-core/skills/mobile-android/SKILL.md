---
name: mobile-android
description: "Native Android with Kotlin and Jetpack Compose: state hoisting and saving, ViewModels with StateFlow, Navigation Compose, lazy lists with keys, architecture layers with Hilt, coroutines and lifecycle-aware collection, Room and DataStore, WorkManager, runtime permissions, edge-to-edge, accessibility, testing, Gradle and R8. Read when the project is a native Android app."
---

# Native Android with Kotlin and Compose

The naive Android app: logic in the Activity, `remember` state lost on
rotation, `collectAsState` collecting while the app sits in the background,
`GlobalScope.launch`, a `LazyColumn` without keys, content under the status
bar once the app targets Android 15, a permission requested in `onCreate`,
tokens in plain SharedPreferences, `kapt`, and a release build that crashes
because R8 removed a class. This skill is current Compose practice.
Conventions in `mobile-ux`, sync in `mobile-data`, Play in `mobile-release`.

## 1. Targets and setup

- Compose first; Views through `AndroidView` and `ComposeView` interop.
- targetSdk and compileSdk at the level Play requires (API 36 for updates
  from August 2026); minSdk 26 for a new app unless the audience needs
  older. Kotlin 2 with the Compose compiler plugin, KSP instead of kapt,
  the Compose BOM, a version catalogue (`gradle/libs.versions.toml`).

```kotlin
android {
  compileSdk = 36
  defaultConfig { applicationId = "com.acme.app"; minSdk = 26; targetSdk = 36 }
  buildTypes {
    release {
      isMinifyEnabled = true
      isShrinkResources = true
      proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
    }
  }
}
```

## 2. State

- Screens are stateless (state in, events out). `remember` survives
  recomposition only; `rememberSaveable` survives rotation and process death
  (keep it well under 50 KB); a ViewModel survives rotation, not process
  death, so input that must survive goes in `SavedStateHandle`.

```kotlin
data class OrdersUiState(val orders: List<Order> = emptyList(), val refreshing: Boolean = false, val refreshFailed: Boolean = false)

@HiltViewModel
class OrdersViewModel @Inject constructor(private val repo: OrderRepository) : ViewModel() {
  private val status = MutableStateFlow(OrdersUiState())
  val uiState: StateFlow<OrdersUiState> =
    combine(repo.observeOrders(), status) { orders, s -> s.copy(orders = orders) }
      .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5_000), OrdersUiState(refreshing = true))

  fun refresh() {
    viewModelScope.launch {
      status.update { it.copy(refreshing = true, refreshFailed = false) }
      val failed = try { repo.refresh(); false } catch (e: IOException) { true } catch (e: HttpException) { true }
      status.update { it.copy(refreshing = false, refreshFailed = failed) }
    }
  }
}

@Composable
fun OrdersRoute(onOpen: (String) -> Unit, viewModel: OrdersViewModel = hiltViewModel()) {
  val state by viewModel.uiState.collectAsStateWithLifecycle()
  OrdersScreen(state, onOpen = onOpen, onRefresh = viewModel::refresh)
}
```

- `WhileSubscribed(5_000)` survives a rotation and stops 5 seconds after
  the screen leaves; `collectAsStateWithLifecycle` pauses in the background.
- One-off results (saved, then navigate) are state the screen reacts to.

## 3. Navigation Compose

```kotlin
@Serializable object OrderList
@Serializable data class OrderDetail(val id: String)

NavHost(navController, startDestination = OrderList) {
  composable<OrderList> { OrdersRoute(onOpen = { navController.navigate(OrderDetail(it)) }) }
  composable<OrderDetail>(
    deepLinks = listOf(navDeepLink<OrderDetail>(basePath = "https://acme.com/orders")),
  ) { OrderDetailRoute() } // its ViewModel reads savedStateHandle.toRoute<OrderDetail>()
}
```

- Type-safe routes (Navigation 2.8+); Navigation 3 for new adaptive apps.
- Bottom navigation keeps each tab's stack: `popUpTo(startDestination) {
  saveState = true }`, `launchSingleTop = true`, `restoreState = true`.
  After sign-in, `popUpTo` the sign-in graph with `inclusive = true`.
- App Links: an intent filter with `android:autoVerify="true"` and
  `/.well-known/assetlinks.json` on the domain.

## 4. Lists and recomposition

```kotlin
LazyColumn(contentPadding = innerPadding) {
  items(state.orders, key = { it.id }, contentType = { "order" }) { order ->
    OrderRow(order, onClick = { onOpen(order.id) }, modifier = Modifier.animateItem())
  }
}
```

- Stable keys keep scroll position and row state through inserts;
  `contentType` lets rows of one type reuse each other.
- Strong skipping (default since Kotlin 2.0.20) skips composables with
  equal parameters and remembers lambdas: keep UI state immutable, never
  pass a ViewModel or `MutableList` down.
- `derivedStateOf` for values derived from fast-changing state; read scroll
  and animation values in layout or draw (`Modifier.offset { }`,
  `graphicsLayer { }`), not in composition. Recomposition counts in the
  Layout Inspector (`mobile-performance`).

## 5. Architecture and Hilt

- UI layer, data layer (repositories as the single source of truth over
  Room and the network), a domain layer only for logic shared between
  ViewModels. Hilt: `@HiltAndroidApp`, `@AndroidEntryPoint`,
  `@HiltViewModel`, `@Binds` in `SingletonComponent` modules.

```kotlin
class OrderRepository @Inject constructor(private val dao: OrderDao, private val api: OrderApi) {
  fun observeOrders(): Flow<List<Order>> = dao.observeAll().map { rows -> rows.map { it.toDomain() } }
  suspend fun refresh() = dao.upsertAll(api.orders().map { it.toEntity() })
}
```

## 6. Coroutines and Flow

- `viewModelScope` and `lifecycleScope`; never `GlobalScope` or
  `runBlocking` on the main thread.
- Room and Retrofit suspend functions are main-safe; other blocking I/O in
  `withContext(Dispatchers.IO)`, CPU work on `Dispatchers.Default`, with
  dispatchers injected for tests.
- Never swallow `CancellationException` (`runCatching` catches it).
- `debounce(300)` with `flatMapLatest` for search, `distinctUntilChanged`,
  `catch`, `retryWhen`; `repeatOnLifecycle(STARTED)` in Views code.

## 7. Persistence and secrets

- Room: DAOs returning `Flow`, `@Upsert`, `@Transaction`; the Room Gradle
  plugin with `schemaDirectory("$projectDir/schemas")` committed;
  auto-migrations for simple changes, `Migration(from, to)` otherwise,
  tested with `MigrationTestHelper`; no destructive fallback over user data.
- DataStore for settings, not SharedPreferences.
- EncryptedSharedPreferences is deprecated. Encrypt tokens with an AES-GCM
  key held in the Android Keystore, and store the result in DataStore:

```kotlin
private fun key(): SecretKey {
  val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
  (ks.getEntry("auth", null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
  return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").apply {
    init(KeyGenParameterSpec.Builder("auth", KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
      .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
      .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
      .setKeySize(256).build())
  }.generateKey()
}

fun encrypt(plain: ByteArray): ByteArray = Cipher.getInstance("AES/GCM/NoPadding").run {
  init(Cipher.ENCRYPT_MODE, key()); iv + doFinal(plain) // 12-byte IV first
}

fun decrypt(data: ByteArray): ByteArray = Cipher.getInstance("AES/GCM/NoPadding").run {
  init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, data, 0, 12)); doFinal(data, 12, data.size - 12)
}
```

- Keystore keys are not backed up: exclude the token store from backup
  (`android:dataExtractionRules`) and treat a failed decryption as signed
  out.

## 8. Background work

```kotlin
val request = PeriodicWorkRequestBuilder<SyncWorker>(1, TimeUnit.HOURS)
  .setConstraints(Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build())
  .setBackoffCriteria(BackoffPolicy.EXPONENTIAL, 30, TimeUnit.SECONDS)
  .build()
WorkManager.getInstance(context).enqueueUniquePeriodicWork("sync", ExistingPeriodicWorkPolicy.KEEP, request)
```

- `SyncWorker` is a `CoroutineWorker` returning success, retry (capped by
  `runAttemptCount`) or failure. Periodic work runs at most every 15
  minutes, Doze delays it, and a worker gets about 10 minutes.
- Foreground services only for work the user started and sees, with a
  declared `foregroundServiceType` and its permission; `dataSync` is capped
  at 6 hours a day from Android 15. Exact alarms need
  `SCHEDULE_EXACT_ALARM`, denied by default from Android 14.

## 9. Permissions

```kotlin
val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
  if (granted) openScanner() else showCameraHelp = true
}
Button(onClick = { launcher.launch(Manifest.permission.CAMERA) }) { Text(stringResource(R.string.scan)) }
```

- At the moment of use; `shouldShowRequestPermissionRationale` says when to
  explain first; after two denials the system stops asking, so offer
  `Settings.ACTION_APPLICATION_DETAILS_SETTINGS`.
- `POST_NOTIFICATIONS` from Android 13, asked when notifications matter.
  Photos through the Photo Picker (`PickVisualMedia`), no permission.
- Location: coarse and fine requested together (the user may grant only
  approximate); background location later, with a Play declaration.

## 10. Edge-to-edge, theming and large screens

- `enableEdgeToEdge()` before `setContent`; `Scaffold` passes
  `innerPadding`; `safeDrawingPadding()` and `imePadding()` elsewhere.
  Enforced when targeting API 35, with no opt-out at 36.
- Material 3 from tokens (`lightColorScheme`, `darkColorScheme`), dynamic
  colour on Android 12+ where the brand allows, `Typography` in `sp`.
- `BackHandler` or `PredictiveBackHandler` only to intercept back; apps
  targeting Android 16 no longer get `onBackPressed`.
- Large screens: `NavigationSuiteScaffold` and `ListDetailPaneScaffold`
  (material3-adaptive). Android 16 ignores orientation and resize locks
  from 600dp for apps targeting API 36.

## 11. Accessibility

`contentDescription` on meaningful icons, `null` on decorative ones;
`Modifier.semantics(mergeDescendants = true)` for rows, `heading()`,
`stateDescription`, `customActions`; `toggleable` and `selectable` bring
roles; 48dp targets (`minimumInteractiveComponentSize()`); text in `sp`.
Walk it with TalkBack and Accessibility Scanner.

## 12. Testing

```kotlin
@get:Rule val compose = createComposeRule()

@Test fun showsRetryWhenRefreshFails() {
  compose.setContent { OrdersScreen(OrdersUiState(refreshFailed = true), onOpen = {}, onRefresh = {}) }
  compose.onNodeWithText("Retry").assertIsDisplayed().performClick()
}
```

- Unit tests with `runTest`, a main-dispatcher rule, Turbine for flows,
  fakes before mocks. Compose tests by text, content description or
  `testTag`, on the JVM with Robolectric or on a device; Room migrations
  and WorkManager (work-testing) as instrumented tests. Screenshots with
  Roborazzi or Paparazzi.

## 13. Gradle, R8 and baseline profiles

- Variants are build types times flavors: `applicationIdSuffix` for dev and
  staging, `buildConfigField` for non-secret values.
- R8 full mode is the default since AGP 8: test the release build, keep
  rules only for classes reached by reflection, and keep each release's
  `mapping.txt` for crash reports (`mobile-release`).
- Baseline profiles (the `androidx.baselineprofile` plugin, a generator
  module, `./gradlew :app:generateBaselineProfile`) precompile startup and
  key journeys; measure with Macrobenchmark before and after.

## Check it

```bash
./gradlew assembleDebug testDebugUnitTest lintDebug
./gradlew connectedDebugAndroidTest     # emulator or device attached
./gradlew assembleRelease               # R8 and resource shrinking
adb shell am kill com.acme.app          # after pressing Home: process death
```

Reopen from recents after the kill: screen, input and scroll position are
back. Rotate, switch to dark mode and font scale 2.0 on changed screens.

## Avoid

Logic in Activities; `GlobalScope` and `runBlocking`; `collectAsState`
without the lifecycle; state only in `remember`; lists without keys;
mutable UI state; content under the system bars; permissions on launch;
tokens in plain preferences; new code on EncryptedSharedPreferences;
`kapt` in new modules; destructive migrations; release builds never run.
