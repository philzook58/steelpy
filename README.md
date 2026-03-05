# steelpy

Python bindings to Steel, a rust embeddable scheme <https://github.com/mattwparas/steel>

Typical python and scheme values transport across fairly transparently.

```python
import steel_py
engine = steel_py.SteelEngine()
assert engine.eval("(+ 1 1)") == 2
```
