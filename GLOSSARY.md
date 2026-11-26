# Bevy Liquid Glass

为 Bevy UI 提供可复用的玻璃外观和背景处理。

## Language

**Glass Node（玻璃节点）**：由 GlassNode 描述、拥有自身形状和光学样式的独立界面表面。其前景文字和交互内容保持清晰。
_Avoid_: 固定演示形状

**Glass Style（玻璃样式）**：决定折射、色散、反光、tint 和阴影的参数集合；独立节点与融合组共用。几何单独由 GlassShape 描述。
_Avoid_: 全局效果设置

**Glass Group（玻璃组）**：把多个玻璃节点融合成一个连续玻璃表面的集合，具有共同的光学外观与阴影。
_Avoid_: 演示形状对

**Glass Shape（玻璃形状）**：圆角半径与超椭圆指数。用于独立节点或组成员。

**Group Member（组成员）**：通过 GlassMember 为最近祖先玻璃组提供自身形状，保留独立的内容与交互；没有独立光学参数。
_Avoid_: 合并后的控件

**Blend Radius（融合半径）**：控制相邻玻璃形状之间过渡范围的距离；为零时形成硬并集，增大时形成平滑连接。
_Avoid_: 背景模糊半径

**Backdrop（背景）**：玻璃节点后方供光学效果采样的画面。
_Avoid_: 示例壁纸

**Capture Boundary（捕获位置）**：决定哪些已绘制的界面内容进入共享背景的位置。
_Avoid_: 每个节点递归截屏
