# Kale

<p align="center">
  <img src="./img/icons/icon1.png" alt="icon" width="200">
</p>

`kale` 将图片转换为高保真终端字符画。每个终端字符单元都拥有独立的 ANSI 24-bit True Color 前景色和背景色；它不是简单地给 ASCII 字符上色，而是为每个字符格拟合最接近原图的两种颜色。

支持 Windows Terminal、WezTerm、kitty、iTerm2，以及现代 VS Code 集成终端等 True Color 终端。

## 快速开始

### 从源码运行

要求 Node.js 18 或更高版本。

```powershell
# 获取源码后进入项目目录
cd kale
npm install
node .\src\cli.js .\photo.png --width 100
```

`npm run start --` 也可以运行，但如果你要把结果重定向到文件，请直接使用 `node .\src\cli.js`，避免 npm 的提示文本混入字符画。

### 打包 Windows 便携版

```powershell
cd D:\Code\kale
npm install
npm run package
```

产物目录为 `dist\kale-win-x64`。它包含 Node.js 运行时、渲染代码和 Sharp 图像解码组件；目标电脑不需要安装 Node.js，但必须完整保留该目录。

```powershell
.\dist\kale-win-x64\kale.cmd .\photo.png --width 100 --mode quadrant
```

这个流程不依赖第三方单文件打包器的运行时缓存，因此可离线重复构建。发布时建议在一台干净的 Windows x64 环境中执行 `npm run package`，再将整个 `dist\kale-win-x64` 文件夹压缩为 ZIP 分发。

```powershell
Compress-Archive -Path .\dist\kale-win-x64\* -DestinationPath .\dist\kale-win-x64.zip -Force
```

## 命令行

```text
kale <image> [options]
```

| 选项 | 说明 | 默认值 |
| --- | --- | --- |
| `-w, --width <columns>` | 字符画宽度，单位为终端列数，范围 1–1000 | `80` |
| `-m, --mode <mode>` | 渲染模式：`half`、`quadrant`、`braille`、`glyph` | `half` |
| `-f, --format <format>` | 输出格式：`ansi` 或 `html` | `ansi` |
| `-b, --background <#rrggbb>` | 透明 PNG/WebP 的合成背景色 | `#000000` |
| `--font <family>` | `glyph` 模式用于校准字形遮罩的字体 | `monospace` |
| `-h, --help` | 显示帮助 | — |

支持由 Sharp 解码的常见格式，例如 PNG、JPEG、WebP、GIF 的静态首帧、TIFF 和 AVIF。

## 渲染模式

| 模式 | 每格源采样 | 最适合 | 特点 |
| --- | --- | --- | --- |
| `half` | 1×2 | 默认 Logo、照片 | `▀` 上下双色块；速度最快、色彩最稳定 |
| `quadrant` | 2×2 | Fastfetch Logo、图标、插画 | Unicode 象限块；边缘更利落 |
| `braille` | 2×4 | 小尺寸图、纹理、线稿 | 盲文点阵；几何密度最高，但取决于终端字体 |
| `glyph` | 8×16 | 希望保留“字符画”风格的作品 | 对真实字符的抗锯齿覆盖率拟合前景/背景色 |

`half`、`quadrant` 和 `braille` 会枚举可用子像素掩码，把笔画覆盖区与未覆盖区各自拟合为一个 RGB 颜色，并选择总误差最小的字符。`glyph` 会先栅格化候选字符，再以最小二乘法求解前景色和背景色。

## 使用示例

### 默认真彩半块

```powershell
node .\src\cli.js .\photo.jpg -w 100
```

### 更适合图标的象限块

```powershell
node .\src\cli.js .\icon.png --width 48 --mode quadrant
```

### 高密度 Braille

```powershell
node .\src\cli.js .\drawing.png -w 90 -m braille
```

### 字形拟合，并匹配 Windows Terminal 常见字体

```powershell
node .\src\cli.js .\photo.png -w 96 -m glyph --font 'Cascadia Mono'
```

### 带透明通道的图像

```powershell
node .\src\cli.js .\logo.png -w 60 -m quadrant --background '#1e293b'
```

### 导出 HTML

```powershell
node .\src\cli.js .\photo.png -w 120 -m quadrant --format html > art.html
```

生成的 `art.html` 是可直接嵌入网页的 `<pre>` 片段，而非完整 HTML 文档。

## Fastfetch 集成

Fastfetch 可以直接显示 ANSI 真彩字符画；一定使用 `file-raw`，它不会篡改文件里的 ANSI 转义序列。

```powershell
node .\src\cli.js .\logo.png -w 36 -m quadrant > $env:APPDATA\fastfetch\chroma-logo.ansi
fastfetch --file-raw $env:APPDATA\fastfetch\chroma-logo.ansi
```

将以下内容加入 Fastfetch 配置文件（可用 `fastfetch --list-config-paths` 找到目录）：

```jsonc
{
  "logo": {
    "type": "file-raw",
    "source": "C:/Users/你的用户名/AppData/Roaming/fastfetch/chroma-logo.ansi",
    "padding": { "right": 2 }
  }
}
```

对 Fastfetch 而言，建议宽度为 30–40 列；通常优先选 `quadrant`。如果 Logo 太高，可降低 `--width`，因为工具会自动按终端字格比例保持图像比例。

## 验证与排错

```powershell
npm test
node .\src\cli.js --help
```

- 输出只有灰色：终端可能不支持 True Color，或在管道/日志查看器中查看了 ANSI 输出。请在 Windows Terminal、WezTerm 或 VS Code 终端中直接运行。
- 画面偏扁或偏高：终端所用字体的宽高比与默认估计不同。`glyph` 模式请用 `--font` 指定你实际使用的等宽字体。
- Fastfetch 显示了控制字符：使用 `--file-raw`，不要使用 `--file`；并确保 `.ansi` 文件由 `node .\src\cli.js` 直接生成。
- 便携版无法读取图片：不要只复制 `kale.cmd` 或 `node.exe`；请完整保留 `kale-win-x64` 文件夹。

## 开发

```powershell
npm install
npm test
npm run package
```

测试覆盖 ANSI 前景/背景输出、颜色序列去重、尺寸换算、HTML、象限块、Braille 与字形遮罩颜色拟合。
