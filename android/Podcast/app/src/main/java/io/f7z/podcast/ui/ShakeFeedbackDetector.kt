package io.f7z.podcast.ui

import android.content.Context
import android.hardware.Sensor
import android.hardware.SensorEvent
import android.hardware.SensorEventListener
import android.hardware.SensorManager
import android.os.SystemClock
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.platform.LocalContext
import kotlin.math.sqrt

/**
 * Event-driven Android shake detector for opening feedback.
 *
 * Native owns the gesture affordance; Rust owns feedback state and protocol
 * behavior. The listener is registered only while the composable is alive and
 * debounces callbacks so repeated accelerometer spikes open one sheet.
 */
@Composable
fun ShakeFeedbackDetector(
    minIntervalMs: Long = 1_000,
    onShake: () -> Unit,
) {
    val context = LocalContext.current
    val latestOnShake = rememberUpdatedState(onShake)

    DisposableEffect(context, minIntervalMs) {
        val sensorManager =
            context.getSystemService(Context.SENSOR_SERVICE) as? SensorManager
        val accelerometer = sensorManager?.getDefaultSensor(Sensor.TYPE_ACCELEROMETER)
        if (sensorManager == null || accelerometer == null) {
            return@DisposableEffect onDispose {}
        }

        var lastShakeAt = 0L
        val listener = object : SensorEventListener {
            override fun onSensorChanged(event: SensorEvent) {
                if (event.sensor.type != Sensor.TYPE_ACCELEROMETER) return
                val x = event.values.getOrNull(0) ?: return
                val y = event.values.getOrNull(1) ?: return
                val z = event.values.getOrNull(2) ?: return
                val gForce = sqrt((x * x + y * y + z * z).toDouble()) /
                    SensorManager.GRAVITY_EARTH
                if (gForce < SHAKE_G_FORCE_THRESHOLD) return

                val now = SystemClock.elapsedRealtime()
                if (now - lastShakeAt < minIntervalMs) return
                lastShakeAt = now
                latestOnShake.value()
            }

            override fun onAccuracyChanged(sensor: Sensor?, accuracy: Int) = Unit
        }

        sensorManager.registerListener(
            listener,
            accelerometer,
            SensorManager.SENSOR_DELAY_UI,
        )
        onDispose { sensorManager.unregisterListener(listener) }
    }
}

private const val SHAKE_G_FORCE_THRESHOLD = 2.7
