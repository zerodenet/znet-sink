# Tauri discovers these entry points and annotations through reflection.
-keepattributes *Annotation*
-keep class org.zerodenet.znetsink.nativeio.NativeIoPlugin { *; }
-keep class org.zerodenet.znetsink.nativeio.NativeIoArgs { *; }
-keepclassmembers class * {
    @app.tauri.annotation.Command <methods>;
    @app.tauri.annotation.ActivityCallback <methods>;
}

# JNI OS job entry points survive release shrinking.
-keep class org.zerodenet.znetsink.nativeio.NativePluginJobs { *; }
-keep class org.zerodenet.znetsink.nativeio.NativePluginJobService { *; }
