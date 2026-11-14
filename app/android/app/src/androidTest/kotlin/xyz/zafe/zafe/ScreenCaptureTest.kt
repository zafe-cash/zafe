package xyz.zafe.zafe

import android.view.WindowManager
import androidx.lifecycle.Lifecycle
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

/** Run on both an API < 33 and an API >= 33 device: their old resume policies differed. */
@RunWith(AndroidJUnit4::class)
class ScreenCaptureTest {
    private fun assertSecure(scenario: ActivityScenario<MainActivity>) {
        scenario.onActivity { activity ->
            assertTrue(
                "Every route must block screenshots and screen recording",
                activity.window.attributes.flags and WindowManager.LayoutParams.FLAG_SECURE != 0,
            )
        }
    }

    @Test
    fun ordinaryScreenBlocksCaptureOnLaunch() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            assertSecure(scenario)
        }
    }

    @Test
    fun returningFromBackgroundKeepsCaptureBlocked() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.moveToState(Lifecycle.State.CREATED)
            scenario.moveToState(Lifecycle.State.RESUMED)
            assertSecure(scenario)
        }
    }

    @Test
    fun recreatedActivityBlocksCapture() {
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.recreate()
            assertSecure(scenario)
        }
    }
}
