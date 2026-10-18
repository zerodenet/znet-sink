import com.android.build.api.dsl.ApplicationExtension
import org.gradle.kotlin.dsl.configure

// Tauri's CLI rewrites applicationId assignments in the main build script.
// Keep the installed Debug identity in an applied script that it does not edit;
// the base namespace and Tauri/JNI identifier remain unchanged.
extensions.configure<ApplicationExtension> {
    buildTypes.getByName("debug").applicationIdSuffix = ".debug"
}
