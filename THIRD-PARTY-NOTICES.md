# Third-party notices

AiPet itself is released under the MIT License (see [LICENSE](LICENSE)).

The release packages are self-contained: the Windows installer and zip and the Linux tarballs include the .NET
runtime and the libraries below, so nothing else needs to be installed. `aipet-hook`, the program the Claude Code
and Codex plugin runs, is compiled ahead of time and contains parts of the .NET runtime. This file lists that
software, its license and its copyright notice. The versions are the ones `src/AiPet.UI/AiPet.UI.csproj` resolves
to, and the licenses and notices are the ones the packages declare.

| Component | Version | What it's for | License | Copyright |
|---|---|---|---|---|
| [Avalonia](https://github.com/AvaloniaUI/Avalonia): Avalonia, Avalonia.Desktop, Avalonia.Themes.Fluent and the packages they bring in (Avalonia.Win32, Avalonia.X11, Avalonia.FreeDesktop, Avalonia.FreeDesktop.AtSpi, Avalonia.Skia, Avalonia.HarfBuzz, Avalonia.Native, Avalonia.Remote.Protocol). The Avalonia package also holds Base, Controls, Markup, Markup.Xaml, OpenGL, Vulkan, Metal, MicroCom, Dialogs and DesignerSupport | 12.1.3 | The app's windows and rendering | MIT | Copyright 2013-2026 © The AvaloniaUI Project |
| [ANGLE](https://github.com/AvaloniaUI/angle) for Windows (Avalonia.Angle.Windows.Natives, `av_libglesv2.dll`) | 2.1.27548.20260419 | OpenGL ES on Direct3D (Windows only) | BSD-3-Clause | Copyright 2018 The ANGLE Project Authors |
| [SkiaSharp](https://github.com/mono/SkiaSharp), with its native `libSkiaSharp` (SkiaSharp.NativeAssets.Win32 and .Linux) | 3.119.4 | 2D drawing | MIT | Copyright (c) 2015-2016 Xamarin, Inc.; Copyright (c) 2017-2018 Microsoft Corporation |
| [HarfBuzzSharp](https://github.com/mono/SkiaSharp), with its native `libHarfBuzzSharp` (HarfBuzzSharp.NativeAssets.Win32 and .Linux) | 8.3.1.3 | Text shaping | MIT | Copyright (c) 2015-2016 Xamarin, Inc.; Copyright (c) 2017-2018 Microsoft Corporation |
| [MicroCom.Runtime](https://github.com/kekekeks/MicroCom) | 0.11.6 | COM interop used by Avalonia | MIT | Copyright 2021 © Nikita Tsukanov |
| [Tmds.DBus.Protocol](https://github.com/tmds/Tmds.DBus) | 0.94.1 | D-Bus, used by Avalonia on Linux | MIT | Copyright Tom Deseyn |
| [Velopack](https://github.com/velopack/velopack) (the `Velopack.dll` library in every package; `Setup.exe` and `Update.exe`, which vpk 1.2.158 adds to the Windows installer) | 1.2.158 | Windows installer, updates and uninstall | MIT | Copyright © Velopack Ltd. |
| [.NET runtime and libraries](https://github.com/dotnet/runtime) (Microsoft.NETCore.App) | 10.0 (the patch that comes with the release build's SDK) | Runs the app; compiled into `aipet-hook` | MIT | Copyright (c) .NET Foundation and Contributors |

Avalonia also brings in Avalonia.BuildServices (11.3.2, MIT), which runs only during the build: nothing of it is
in the packages. SkiaSharp's and HarfBuzzSharp's macOS and WebAssembly native packages add no files to the
Windows and Linux packages.

## Software inside the native libraries

`libSkiaSharp` and `libHarfBuzzSharp` are built from [Skia](https://skia.org/) (BSD-3-Clause, Google) and
[HarfBuzz](https://github.com/harfbuzz/harfbuzz) (Old MIT), together with libraries such as FreeType, libpng,
libjpeg-turbo, libwebp, expat, ICU and zlib. Microsoft publishes the notices for all of them in
`THIRD-PARTY-NOTICES.txt` inside the `SkiaSharp.NativeAssets.*` and `HarfBuzzSharp.NativeAssets.*` packages
on nuget.org (versions 3.119.4 and 8.3.1.3).

Velopack's `Setup.exe` and `Update.exe` are written in Rust and built from crates that carry their own licenses.

The .NET runtime includes third-party code as well. Its notices are in
[THIRD-PARTY-NOTICES.TXT](https://github.com/dotnet/runtime/blob/main/THIRD-PARTY-NOTICES.TXT) in the dotnet/runtime
repository, and in the `Microsoft.NETCore.App.Runtime.<rid>` packages.

## License texts

### MIT License

Used by Avalonia, SkiaSharp, HarfBuzzSharp, MicroCom.Runtime, Tmds.DBus.Protocol, the .NET runtime and
Velopack, each with the copyright notice given in the table above.

```
Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

### BSD 3-Clause License (ANGLE)

```
Copyright 2018 The ANGLE Project Authors.
All rights reserved.

Redistribution and use in source and binary forms, with or without
modification, are permitted provided that the following conditions
are met:

    Redistributions of source code must retain the above copyright
    notice, this list of conditions and the following disclaimer.

    Redistributions in binary form must reproduce the above
    copyright notice, this list of conditions and the following
    disclaimer in the documentation and/or other materials provided
    with the distribution.

    Neither the name of TransGaming Inc., Google Inc., 3DLabs Inc.
    Ltd., nor the names of their contributors may be used to endorse
    or promote products derived from this software without specific
    prior written permission.

THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS
"AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT
LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS
FOR A PARTICULAR PURPOSE ARE DISCLAIMED. IN NO EVENT SHALL THE
COPYRIGHT OWNER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT,
INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING,
BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR SERVICES;
LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT
LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN
ANY WAY OUT OF THE USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE
POSSIBILITY OF SUCH DAMAGE.
```
