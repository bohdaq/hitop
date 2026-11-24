# hitop-ffi's JNI function is found by this class and method name.
-keep class tech.rws8.hitop.ffi.Native { *; }
# The app is open source: names stay, so crash reports read without a mapping file.
-dontobfuscate
-keepattributes SourceFile,LineNumberTable
