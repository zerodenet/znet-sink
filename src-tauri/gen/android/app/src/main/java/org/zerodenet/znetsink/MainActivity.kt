package org.zerodenet.znetsink

import android.os.Bundle
import android.content.Intent
import android.net.Uri
import android.provider.Settings
import android.util.TypedValue
import android.view.ViewGroup
import android.widget.Button
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TextView
import android.view.View
import android.webkit.WebView
import androidx.activity.OnBackPressedCallback
import androidx.activity.enableEdgeToEdge
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import androidx.core.view.WindowInsetsControllerCompat
import androidx.core.view.doOnAttach
import androidx.webkit.WebViewCompat
import kotlin.math.max

class MainActivity : TauriActivity() {
  private var hostWebView: WebView? = null
  private var backRequestPending = false
  private var keyboardVisible = false
  private var compatibilityView: ScrollView? = null

  override fun onCreate(savedInstanceState: Bundle?) {
    enableEdgeToEdge()
    super.onCreate(savedInstanceState)
    // Android 7's WebView does not supply CSS safe-area variables. Native layout owns
    // system bars/cutouts/IME bounds, so forms and the fixed navigation stay reachable.
    val content = findViewById<View>(android.R.id.content)
    ViewCompat.setOnApplyWindowInsetsListener(content) { view, insets ->
      val bars = insets.getInsets(WindowInsetsCompat.Type.systemBars() or WindowInsetsCompat.Type.displayCutout())
      val keyboard = insets.getInsets(WindowInsetsCompat.Type.ime())
      keyboardVisible = insets.isVisible(WindowInsetsCompat.Type.ime())
      view.setPadding(bars.left, bars.top, bars.right, max(bars.bottom, keyboard.bottom))
      insets
    }
    content.doOnAttach { ViewCompat.requestApplyInsets(it) }
    onBackPressedDispatcher.addCallback(this, object : OnBackPressedCallback(true) {
      override fun handleOnBackPressed() {
        if (compatibilityView != null) { finish(); return }
        if (keyboardVisible) {
          WindowInsetsControllerCompat(window, content).hide(WindowInsetsCompat.Type.ime())
          return
        }
        if (backRequestPending) return
        val webView = hostWebView
        if (webView == null) { finish(); return }
        backRequestPending = true
        try {
          // The host shell closes its top dialog or source page and cancels the event
          // when handled. A root page leaves normal Android back/exit behavior intact.
          webView.evaluateJavascript("window.dispatchEvent(new CustomEvent('mobile:back', {cancelable: true}))") { result ->
            backRequestPending = false
            if (result != "false" && !isFinishing && !isDestroyed) finish()
          }
        } catch (_: RuntimeException) {
          backRequestPending = false
          finish()
        }
      }
    })
  }

  override fun onWebViewCreate(webView: WebView) {
    super.onWebViewCreate(webView)
    hostWebView = webView
    if (!WebViewCompatibility.supported(activeAgent(webView))) {
      // Wry calls this hook BEFORE its URL load/setContentView. Disable modern JS now,
      // then attach the native explanation after Wry has installed the content view.
      webView.settings.javaScriptEnabled = false
      webView.visibility = View.INVISIBLE
      webView.doOnAttach { it.post { showCompatibilityGuard(webView) } }
    }
  }

  private fun showCompatibilityGuard(webView: WebView) {
    if (isFinishing || isDestroyed || hostWebView !== webView) return
    val major = WebViewCompatibility.chromiumMajor(activeAgent(webView))
    if (major != null && major >= WebViewCompatibility.MINIMUM_CHROMIUM_MAJOR) {
      compatibilityView?.let { (it.parent as? ViewGroup)?.removeView(it) }
      compatibilityView = null
      webView.settings.javaScriptEnabled = true
      webView.visibility = View.VISIBLE
      webView.reload()
      return
    }
    compatibilityView?.let { (it.parent as? ViewGroup)?.removeView(it) }
    val content = findViewById<ViewGroup>(android.R.id.content)
    val provider = try { WebViewCompat.getCurrentWebViewPackage(this) } catch (_: RuntimeException) { null }
    val panel = LinearLayout(this).apply {
      orientation = LinearLayout.VERTICAL
      setPadding(dp(20), dp(20), dp(20), dp(20))
    }
    fun text(value: String, size: Float) = TextView(this).apply {
      text = value
      setTextSize(TypedValue.COMPLEX_UNIT_SP, size)
      setPadding(0, dp(8), 0, dp(8))
      panel.addView(this, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }
    text(if (major == null) "WebView 兼容性未确认" else "需要更新系统 WebView", 22f)
    text("最低系统为 Android 7.0 / API 24；当前界面需要 Chromium 111 或以上。" +
      if (major == null) "系统未提供可识别的引擎版本，界面暂未加载。" else "当前引擎为 Chromium $major，界面暂未加载。", 16f)
    text(provider?.let { "当前系统提供器：${it.packageName}，版本 ${it.versionName ?: "未提供"}" }
      ?: "系统未提供 WebView 来源，不能猜测应更新哪个应用。", 14f)
    val feedback = text("更新当前 WebView 提供器后重试。已经加载的引擎可能需要关闭应用并重新打开；Android 系统版本本身不能确认界面兼容性。", 16f)
    fun button(label: String, action: () -> Unit) = Button(this).apply {
      text = label
      isAllCaps = false
      minHeight = dp(48)
      setTextSize(TypedValue.COMPLEX_UNIT_SP, 16f)
      setOnClickListener { action() }
      panel.addView(this, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }
    button("打开当前 WebView 应用详情") {
      val current = try { WebViewCompat.getCurrentWebViewPackage(this) } catch (_: RuntimeException) { null }
      if (current == null) {
        feedback.text = "系统未确认提供器，请在系统设置检查 WebView 更新与默认提供器。"
      } else {
        try {
          startActivity(Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS,
            Uri.fromParts("package", current.packageName, null)))
        } catch (_: RuntimeException) {
          feedback.text = "系统未打开应用详情，请从系统设置或当前发行渠道更新已确认的提供器。"
        }
      }
    }.isEnabled = provider != null
    button("重试检查") { showCompatibilityGuard(webView) }
    button("关闭应用界面") { finish() }
    val scroll = ScrollView(this).apply { isFillViewport = true; addView(panel) }
    compatibilityView = scroll
    content.addView(scroll, ViewGroup.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.MATCH_PARENT))
  }

  private fun activeAgent(webView: WebView): String =
    try { webView.settings.userAgentString.orEmpty() } catch (_: RuntimeException) { "" }

  private fun dp(value: Int): Int = (value * resources.displayMetrics.density + 0.5f).toInt()

  override fun onDestroy() {
    compatibilityView = null
    hostWebView = null
    super.onDestroy()
  }
}
