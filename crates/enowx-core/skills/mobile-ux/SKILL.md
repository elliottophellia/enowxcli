---
name: mobile-ux
description: "Mobile interfaces that feel native: iOS and Android conventions for navigation, tab bars and back behaviour, safe areas, touch targets, dynamic type, dark mode, haptics, gestures, forms and keyboards, permission prompts, loading and empty states, lists, and screen reader support. Read before designing or building mobile screens."
---

# Mobile screens that feel native

The generated mobile screen is a web page at phone width: a hamburger menu
instead of a tab bar, a custom back arrow that the Android back gesture
skips past, text frozen at one size, a toast on iOS, a permission dialog on
first launch, a spinner over everything, and icon buttons the screen reader
calls "button". This skill gives the platform conventions and numbers that
make an app feel like it belongs on the phone. Design principles live in
`ui`; native animation APIs in `motion-stacks`.

## 1. Two platforms, two sets of habits

| Element | iOS (Human Interface Guidelines) | Android (Material 3) |
|---|---|---|
| Top of screen | Navigation bar; large title that collapses on scroll; back chevron top left | Top app bar, title at the left; Up arrow top left |
| Back | Edge swipe from the left; no system button | System back gesture from either edge or the back button, with predictive back |
| Main navigation | Tab bar at the bottom, 3 to 5 tabs, icon and label | Navigation bar at the bottom, 3 to 5 destinations; a rail on tablets |
| Primary action | A button in the navigation bar or in the content | A floating action button at the bottom right, or in the content |
| Dialogs | Alert with two buttons side by side, Cancel at the left, destructive in red; action sheet for choices | Dialog with text buttons at the bottom right, confirm rightmost; bottom sheet for choices |
| Short feedback | No toast: inline status or a banner | Snackbar at the bottom, with an action such as Undo |
| Type | SF Pro, Dynamic Type text styles, Body 17pt | Roboto or the brand font, Material type scale, Body Large 16sp |
| Menus | Context menu on long press, pull-down menus on buttons | Dropdown menus; long press starts selection |

Share the brand: colour, type, illustration, content, information
architecture. Adapt the mechanics: navigation, back, dialogs, sheets, date
pickers, switches, the share sheet, haptics. Native code and React Native
get this from system components; Flutter needs Cupertino or `.adaptive`
widgets where it matters (`mobile-flutter`). Since iOS 26, standard bars,
tab bars and sheets use the Liquid Glass material when built with Xcode 26;
custom chrome looks dated beside them, so keep it rare.

## 2. Navigation

- A tab bar with 3 to 5 labelled top-level destinations. Each tab keeps its
  own stack; tapping the active tab pops to its root (and on iOS scrolls to
  the top). Past 5, rethink the structure before adding a "More" tab or a
  drawer.
- Stacks within tabs for drilling down. Hide the tab bar only for full-screen
  tasks (camera, player, a checkout).
- Modals (sheets) for self-contained tasks with their own Cancel and Done:
  compose, create, filter, sign in. Never for drilling down. iOS sheets with
  medium and large detents keep context visible; a swipe down with unsaved
  changes asks first (`interactiveDismissDisabled` plus a confirmation).
- Back always works. On Android, back closes the sheet, then pops the stack,
  then leaves the app from the start destination, with no "press back again
  to exit". Apps targeting Android 16 no longer receive `onBackPressed`:
  intercept only for unsaved changes, with `OnBackPressedCallback` or
  Compose `BackHandler`.
- Every important screen has a deep link, and a cold start from one builds
  the stack under it (an order opened from a notification has the orders
  list beneath, so back works). Universal Links (iOS,
  `apple-app-site-association`) and App Links (Android, `assetlinks.json`
  with `autoVerify`); custom schemes only for development and callbacks.
- Onboarding: 0 to 3 skippable screens. Let people use what needs no
  account before asking them to sign up.

## 3. Layout on a phone

- **Safe areas**: content inside them, backgrounds may extend under the
  status bar and home indicator. Never hard-code their heights (they differ
  per device). SwiftUI respects them by default (`.ignoresSafeArea()` only on
  backgrounds, `.safeAreaInset(edge: .bottom)` for a bottom bar); Compose
  `Scaffold` passes `innerPadding`; React Native `useSafeAreaInsets()` from
  react-native-safe-area-context; Flutter `SafeArea`.
- **Edge-to-edge on Android** is enforced for apps targeting Android 15
  (API 35), with no opt-out from API 36: draw behind the system bars and pad
  content with the insets (`mobile-android`).
- **Reach**: primary actions in the lower half (tab bar, a bottom button, a
  FAB). The top corners are the hardest to reach one-handed; keep frequent
  actions away from them and destructive ones away from where thumbs rest.
- **Targets**: 44 by 44pt on iOS, 48 by 48dp on Android, 8dp apart. A 24dp
  icon gets a 48dp hit area (`hitSlop`, `minimumInteractiveComponentSize`).
- **Spacing**: 16pt or dp side margins on phones, an 8-point grid with 4 for
  small steps. Layout principles in `ui`, `ui-layout`; do not bring web
  breakpoints.
- **Large screens**: Material window size classes: compact under 600dp,
  medium 600 to 839dp, expanded 840dp and up. List and detail side by side
  from expanded; a navigation rail instead of the bottom bar from medium.
  Android 16 ignores orientation locks on screens 600dp and wider for apps
  targeting API 36, and iPad windows resize freely: layouts must adapt.

## 4. Type and Dynamic Type

| iOS style (default size) | pt | Material 3 role | sp |
|---|---|---|---|
| Large Title | 34 | Headline Large | 32 |
| Title 1 / Title 2 / Title 3 | 28 / 22 / 20 | Headline Small / Title Large | 24 / 22 |
| Headline (semibold) / Body | 17 / 17 | Title Medium / Body Large | 16 / 16 |
| Callout / Subheadline | 16 / 15 | Body Medium | 14 |
| Footnote / Caption 1 / Caption 2 | 13 / 12 / 11 | Body Small / Label Small | 12 / 11 |

- Use text styles, not fixed sizes: SwiftUI `.font(.body)` or
  `.custom("Brand", size: 17, relativeTo: .body)`; Compose `sp` units
  through `MaterialTheme.typography`; React Native scales by default
  (`maxFontSizeMultiplier` only on tight chrome such as tab labels);
  Flutter reads `MediaQuery.textScalerOf`, never clamped to 1 for body text.
- The largest sizes are big: iOS accessibility sizes take Body from 17 to
  53pt, Android font scale reaches 200%. Text wraps (no fixed heights, no
  single-line body); rows of label and value stack vertically at
  accessibility sizes (`dynamicTypeSize.isAccessibilitySize`,
  `ViewThatFits`); icons beside text scale with it (`@ScaledMetric`).
- Truncate only where the full text is one tap away.

## 5. Colour, dark mode, haptics and gestures

- System semantic colours adapt for free: iOS `Color(.systemBackground)`,
  `.primary`, `.secondary`, asset catalogue colours with a dark appearance;
  Material colour roles (`surface`, `onSurface`, `surfaceContainer`,
  `primary`), dynamic colour on Android 12+ where the brand allows.
- Dark mode is a designed theme, not an inversion: elevated surfaces are
  lighter, saturated brand colours are toned down, logos and images are
  checked on dark. Contrast 4.5:1 for text and 3:1 for large text and
  controls, in both themes.
- Haptics sparingly: success or failure of a real action, a selection
  changing in a picker, a threshold crossed (a swipe action committing).
  Never on every tap. SwiftUI `.sensoryFeedback(.success, trigger: saved)`;
  Compose `LocalHapticFeedback.current.performHapticFeedback(...)`;
  expo-haptics `Haptics.notificationAsync(...)`; Flutter
  `HapticFeedback.selectionClick()`.
- Every gesture has a visible alternative: swipe to delete and an Edit mode
  or menu; long press and a visible "More" button; pinch and zoom buttons.
  Never take the system edges: the left edge on iOS, both side edges and
  the bottom on Android (gesture exclusion is capped at 200dp per edge).

## 6. Forms and keyboards

- The keyboard per field (email, number pad, phone, URL), no autocapitalise
  or autocorrect on emails, codes and usernames. Return key: Next moves to
  the next field, the last one says Done, Go or Send and submits.
- Autofill on every field it knows: sign-in, new password, one-time code,
  name, address, phone. Paste allowed in password fields, a show toggle,
  passkeys when the backend has them (`backend-auth`).

```swift
TextField("Email", text: $email)
  .keyboardType(.emailAddress).textContentType(.emailAddress)
  .textInputAutocapitalization(.never).autocorrectionDisabled()
  .submitLabel(.next).onSubmit { focus = .password }
```

```kotlin
OutlinedTextField(state = emailState, label = { Text("Email") }, // emailState = rememberTextFieldState()
  keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email, imeAction = ImeAction.Next),
  modifier = Modifier.semantics { contentType = ContentType.EmailAddress })
```

```tsx
<TextInput value={email} onChangeText={setEmail} keyboardType="email-address"
  autoComplete="email" textContentType="emailAddress" autoCapitalize="none"
  returnKeyType="next" onSubmitEditing={() => passwordRef.current?.focus()} />
```

```dart
TextFormField(controller: email, keyboardType: TextInputType.emailAddress,
  autofillHints: const [AutofillHints.email], textInputAction: TextInputAction.next)
```

- The keyboard never covers the focused field or the submit button: SwiftUI
  scroll views adjust by themselves; Compose `Modifier.imePadding()`; React
  Native react-native-keyboard-controller (or `KeyboardAvoidingView` with
  `behavior="padding"` on iOS); Flutter `Scaffold` resizes by default.
- Scrolling dismisses the keyboard (`.scrollDismissesKeyboard(.interactively)`,
  `keyboardDismissMode="on-drag"`); taps on buttons still register
  (`keyboardShouldPersistTaps="handled"`).
- Native date and time pickers, never typed dates. Validation rules in
  `frontend-forms`, field visuals in `ui-part-forms`.

## 7. Permission prompts

- iOS shows each system prompt once per install; after "Don't Allow" only
  Settings can change it. Android stops showing the dialog after two denials
  (Android 11+).
- At the moment of use, a short in-app explanation first ("Allow the camera
  to scan receipts", Continue, Not now), then the system prompt. Purpose
  strings (`NSCameraUsageDescription`) say concretely what for.
- Prefer what needs no permission: the system photo picker (`PhotosPicker`,
  Android Photo Picker), the document picker, the share sheet.
- Location: When In Use first, Always later and only for a feature that
  needs it; approximate location is enough for many features.
- Notifications: ask after the user does something that benefits (follows
  an order). iOS provisional authorisation delivers quietly without a prompt.
- Denied: the feature explains what it would do and offers "Open Settings"
  (`UIApplication.openSettingsURLString`, `Settings.ACTION_APPLICATION_DETAILS_SETTINGS`,
  `Linking.openSettings()`, permission_handler `openAppSettings()`), and
  checks again when the app returns to the foreground.
- App Tracking Transparency only when you really track across apps.

## 8. Loading, empty, error and offline

- Skeletons shaped like the content for lists and details; old data stays
  visible during a refresh; no full-screen spinner over content; a progress
  bar for long, measurable work (`ui-part-loading`).
- Pull to refresh on lists that change (`.refreshable`, `PullToRefreshBox`,
  `RefreshControl`, `RefreshIndicator`).
- Optimistic updates for small reversible actions (like, mark read,
  reorder), rolled back with a message when the server refuses.
- Empty states say what goes here and offer the action that fills it
  (`ContentUnavailableView` on iOS 17+).
- Errors say what failed and offer Retry. Offline is a banner, not a modal,
  with the time of the last update; queued changes show as pending.
- Destructive actions: Undo in a snackbar or banner beats a confirmation
  dialog for anything recoverable.

## 9. Lists

- Native list components: SwiftUI `List` with sections, `.swipeActions`,
  `.searchable`; Compose `LazyColumn` with `stickyHeader`; FlashList or
  `SectionList`; Flutter `ListView.builder` and slivers.
- Row heights: at least 44pt on iOS; Material list items 56dp for one line,
  72dp for two, 88dp for three.
- Swipe actions: trailing for destructive (full swipe deletes on iOS, with
  Undo), leading for a positive action; the same actions in a context menu
  or long press.
- Load the next page 5 to 10 rows before the end, a footer spinner while it
  loads, and a quiet end-of-list state.

## 10. Screen readers

- VoiceOver and TalkBack read a label, a role or trait, a value and a hint.
  Every icon button has a label; decorative images are hidden; section
  titles are headings.
- A row reads as one element with its actions attached, not five stops:

```swift
OrderRow(order: order)
  .accessibilityElement(children: .combine)
  .accessibilityAction(named: "Archive") { archive(order) }
```

```kotlin
Row(Modifier.semantics(mergeDescendants = true) {
  customActions = listOf(CustomAccessibilityAction("Archive") { onArchive(); true })
}) { /* row content */ }
```

- React Native: `accessibilityRole`, `accessibilityLabel`,
  `accessibilityState`, `accessibilityActions`; Flutter: `Semantics`,
  `MergeSemantics`, `tooltip` on `IconButton`.
- Focus follows reading order; after navigating it lands on the title; after
  a sheet closes it returns to what opened it. Announce results that appear
  elsewhere (`AccessibilityNotification.Announcement("Saved").post()` on
  iOS 17+, `liveRegion` in Compose).
- Reduced motion and reduced transparency respected (`motion-comfort`);
  colour never the only signal.

## Check it

- Largest text and dark mode: `xcrun simctl ui booted content_size
  accessibility-extra-extra-extra-large`, `xcrun simctl ui booted appearance
  dark`; `adb shell settings put system font_scale 2.0`, `adb shell cmd
  uimode night yes`. Every screen still readable, nothing clipped.
- Walk every screen with VoiceOver (Accessibility Inspector on the
  simulator) and TalkBack: each stop has a sensible label and role.
- Press Android back from every screen, sheet and dialog; swipe back on iOS.
- Fill each form on the smallest screen with the keyboard up.
- Deny each permission, then grant it from Settings:
  `xcrun simctl privacy booted reset all com.acme.app`,
  `adb shell pm revoke com.acme.app android.permission.CAMERA`.

## Avoid

A hamburger menu for four destinations; custom back buttons that break the
gesture; toasts on iOS; fixed font sizes and single-line body text;
permissions asked on launch or without context; dead ends after a denial;
spinners over whole screens; swipe-only actions; haptics on every tap;
unlabelled icon buttons; the same web layout on both platforms.
