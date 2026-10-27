<!-- loom METADATA -->

# Hostile document

<script>alert(1)</script>

Inline <script>alert(2)</script> and <img src=x onerror=alert(1)> and <a href="javascript:alert(1)">raw</a>.

<iframe src="https://evil.example"></iframe>

<object data="https://evil.example/x.swf"></object>

[js1](<javascript:alert(1)>)

[js2](<JaVaScRiPt:alert(1)>)

[js3](data:text/html,<script>)

[js4](vbscript:msgbox)

[js5](file:///etc/passwd)

[js10][ref]

[ref]: javascript:alert(1)

[ext](//evil.example)

Autolink <https://x.example> here.

Bare autolink <https://example.com> here.

![t](https://tracker.example/p.gif)

| Cell                           |
| ------------------------------ |
| <b onclick="alert(1)">cell</b> |

Code span `<script>alert(3)</script>` here.

[gone](missing.md)

<!-- END loom METADATA -->
