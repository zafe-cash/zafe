import Flutter
import UIKit

class SceneDelegate: FlutterSceneDelegate {
  override func sceneWillResignActive(_ scene: UIScene) {
    super.sceneWillResignActive(scene)
    ZafePrivacy.shared.setInactive(true, window: window)
  }

  override func sceneDidBecomeActive(_ scene: UIScene) {
    super.sceneDidBecomeActive(scene)
    ZafePrivacy.shared.setInactive(false, window: window)
  }
}
