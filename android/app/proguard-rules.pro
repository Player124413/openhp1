# Proguard rules for OpenHP1 NativeActivity
-keep class android.app.NativeActivity { *; }
-keepclassmembers class * {
    native <methods>;
}
