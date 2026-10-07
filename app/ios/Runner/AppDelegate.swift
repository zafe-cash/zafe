import AVFoundation
import Flutter
import UIKit
import workmanager_apple

@main
@objc class AppDelegate: FlutterAppDelegate, FlutterImplicitEngineDelegate {
  override func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions launchOptions: [UIApplication.LaunchOptionsKey: Any]?
  ) -> Bool {
    ZafeBackup.excludeStateFromBackups()

    // Background vault checks (spec §6.1). The identifier is the Dart task's unique name and
    // is listed in Info.plist (BGTaskSchedulerPermittedIdentifiers). The system decides
    // when it actually runs; 15 minutes is the minimum.
    WorkmanagerPlugin.registerLaunchHandlers()
    WorkmanagerPlugin.registerPeriodicTask(
      withIdentifier: "zafe-vault-check", frequency: NSNumber(value: 15 * 60))

    return super.application(application, didFinishLaunchingWithOptions: launchOptions)
  }

  func didInitializeImplicitFlutterEngine(_ engineBridge: FlutterImplicitEngineBridge) {
    GeneratedPluginRegistrant.register(with: engineBridge.pluginRegistry)
    if let registrar = engineBridge.pluginRegistry.registrar(forPlugin: "ZafeNative") {
      ZafeNative.shared.register(messenger: registrar.messenger())
    }
  }
}

/// Keeps single-use FROST nonces, key material and the wallet cache out of iCloud and
/// device backups: restoring an old copy could make a member sign twice with the same
/// nonces (spec §9.4). Android does the same in `data_extraction_rules.xml`.
enum ZafeBackup {
  static func excludeStateFromBackups() {
    let fm = FileManager.default
    let roots: [FileManager.SearchPathDirectory] = [.applicationSupportDirectory, .documentDirectory]
    for root in roots {
      guard var url = fm.urls(for: root, in: .userDomainMask).first else { continue }
      // The directory may not exist yet on a first launch; the flag sticks to the path.
      try? fm.createDirectory(at: url, withIntermediateDirectories: true)
      var values = URLResourceValues()
      values.isExcludedFromBackup = true
      try? url.setResourceValues(values)
    }
  }
}

/// The app's own method channels, the iOS counterparts of MainActivity.kt:
/// `xyz.zafe/payment_feedback`, `secure_screen`, `window_appearance`, `system_settings`
/// and `haptics`.
final class ZafeNative {
  static let shared = ZafeNative()

  private var players: [String: AVAudioPlayer] = [:]
  private let moments = ["approve", "ready", "sent", "received", "failed"]

  /// Haptic taps per payment moment, in ms: the same tempo as the sound's strikes
  /// (scripts/sounds/sounds.py), like the Android handler.
  private let paymentTaps: [String: [Int]] = [
    "approve": [0],
    "ready": [0, 120],
    "sent": [0, 130, 260],
    "received": [0, 110],
    "failed": [0, 120],
  ]

  func register(messenger: FlutterBinaryMessenger) {
    // Ambient: follows the silent switch and mixes with the user's music.
    try? AVAudioSession.sharedInstance().setCategory(.ambient, options: [.mixWithOthers])
    loadPaymentSounds()

    channel("xyz.zafe/payment_feedback", messenger) { [weak self] call, result in
      guard call.method == "play", let args = call.arguments as? [String: Any] else {
        return result(FlutterMethodNotImplemented)
      }
      let moment = args["moment"] as? String ?? ""
      let sound = args["sound"] as? Bool ?? true
      result(self?.playPayment(moment, sound: sound) ?? false)
    }

    channel("xyz.zafe/secure_screen", messenger) { call, result in
      guard call.method == "setSecure" else { return result(FlutterMethodNotImplemented) }
      ZafePrivacy.shared.setSecureRequested(call.arguments as? Bool ?? false)
      result(nil)
    }

    channel("xyz.zafe/window_appearance", messenger) { call, result in
      guard call.method == "setBrightness" else { return result(FlutterMethodNotImplemented) }
      let args = call.arguments as? [String: Any]
      let style: UIUserInterfaceStyle
      switch args?["brightness"] as? String {
      case "dark": style = .dark
      case "light": style = .light
      default: style = .unspecified
      }
      for scene in UIApplication.shared.connectedScenes {
        for window in (scene as? UIWindowScene)?.windows ?? [] {
          window.overrideUserInterfaceStyle = style
        }
      }
      result(nil)
    }

    // iOS has no page for the passcode alone: open the app's own settings page, which is
    // the closest the system allows.
    channel("xyz.zafe/system_settings", messenger) { call, result in
      guard call.method == "openSecurity" else { return result(FlutterMethodNotImplemented) }
      guard let url = URL(string: UIApplication.openSettingsURLString) else {
        return result(false)
      }
      UIApplication.shared.open(url) { opened in result(opened) }
    }

    channel("xyz.zafe/haptics", messenger) { call, result in
      guard call.method == "error" else { return result(FlutterMethodNotImplemented) }
      UINotificationFeedbackGenerator().notificationOccurred(.error)
      result(true)
    }
  }

  private func channel(
    _ name: String, _ messenger: FlutterBinaryMessenger,
    _ handler: @escaping FlutterMethodCallHandler
  ) {
    FlutterMethodChannel(name: name, binaryMessenger: messenger).setMethodCallHandler(handler)
  }

  // MARK: Payment sounds and haptics

  private func loadPaymentSounds() {
    for moment in moments where players[moment] == nil {
      let key = FlutterDartProject.lookupKey(forAsset: "assets/sounds/pay_\(moment).m4a")
      guard let path = Bundle.main.path(forResource: key, ofType: nil),
        let player = try? AVAudioPlayer(contentsOf: URL(fileURLWithPath: path))
      else { continue }
      player.prepareToPlay()
      players[moment] = player
    }
  }

  /// Plays a payment moment: the sound unless it's off (the ambient session is silent on
  /// the silent switch) and the haptic taps. False for an unknown moment.
  private func playPayment(_ moment: String, sound: Bool) -> Bool {
    guard let taps = paymentTaps[moment] else { return false }
    if sound, let player = players[moment] {
      player.currentTime = 0
      player.play()
    }
    if moment == "failed" {
      UINotificationFeedbackGenerator().notificationOccurred(.error)
    } else {
      let generator = UIImpactFeedbackGenerator(style: .medium)
      generator.prepare()
      for at in taps {
        DispatchQueue.main.asyncAfter(deadline: .now() + .milliseconds(at)) {
          generator.impactOccurred()
        }
      }
    }
    return true
  }
}

/// The cover over the app: shown while the app isn't active (the app switcher's snapshot
/// shows the launch screen instead of balances) and while the screen is being recorded or
/// mirrored on a screen that asked to be secret. iOS can't block a screenshot, so the
/// recording cover is the nearest equivalent of Android's FLAG_SECURE.
final class ZafePrivacy {
  static let shared = ZafePrivacy()

  private var cover: UIView?
  private var inactive = false
  private var secureRequested = false

  private init() {
    NotificationCenter.default.addObserver(
      forName: UIScreen.capturedDidChangeNotification, object: nil, queue: .main
    ) { [weak self] _ in self?.update() }
  }

  func setInactive(_ value: Bool, window: UIWindow?) {
    inactive = value
    update(window: window)
  }

  func setSecureRequested(_ value: Bool) {
    secureRequested = value
    update()
  }

  private var captured: Bool { secureRequested && UIScreen.main.isCaptured }

  private func update(window: UIWindow? = nil) {
    let needed = inactive || captured
    if needed {
      guard cover == nil, let window = window ?? Self.keyWindow() else { return }
      let view = UIView(frame: window.bounds)
      view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
      view.backgroundColor = UIColor(named: "LaunchBackground") ?? .black
      if let image = UIImage(named: "LaunchImage") {
        let mark = UIImageView(image: image)
        mark.center = CGPoint(x: view.bounds.midX, y: view.bounds.midY)
        mark.autoresizingMask = [
          .flexibleLeftMargin, .flexibleRightMargin, .flexibleTopMargin, .flexibleBottomMargin,
        ]
        view.addSubview(mark)
      }
      window.addSubview(view)
      cover = view
    } else {
      cover?.removeFromSuperview()
      cover = nil
    }
  }

  private static func keyWindow() -> UIWindow? {
    UIApplication.shared.connectedScenes
      .compactMap { $0 as? UIWindowScene }
      .flatMap { $0.windows }
      .first { $0.isKeyWindow }
  }
}
