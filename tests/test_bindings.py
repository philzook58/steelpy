import unittest
from fractions import Fraction

import steel_py


class SteelPyBindingsTests(unittest.TestCase):
    def test_eval_returns_result(self) -> None:
        result = steel_py.eval("(+ 1 2)")
        self.assertEqual(result, 3)

    def test_eval_converts_useful_defaults(self) -> None:
        self.assertEqual(steel_py.eval('"hello"'), "hello")
        self.assertEqual(steel_py.eval("(list 1 2 3)"), [1, 2, 3])
        self.assertIsNone(steel_py.eval("(define local-x 10)"))

    def test_engine_reuses_state(self) -> None:
        engine = steel_py.SteelEngine()
        self.assertIsNone(engine.eval("(define counter 10)"))
        self.assertEqual(engine.eval("(+ counter 5)"), 15)
        self.assertEqual(engine.eval("(begin (set! counter (+ counter 2)) counter)"), 12)

    def test_eval_with_bindings(self) -> None:
        result = steel_py.eval("(+ x y)", {"x": 4, "y": 7})
        self.assertEqual(result, 11)

    def test_engine_set_and_call(self) -> None:
        engine = steel_py.SteelEngine()
        engine.eval("(define (add3 a b c) (+ a b c))")
        self.assertEqual(engine.call("add3", 1, 2, 3), 6)

        engine.set("py_name", "steel")
        self.assertEqual(engine.eval("py_name"), "steel")

    def test_nested_python_values_to_steel(self) -> None:
        bindings = {"payload": [1, {"ok": True}, "x"]}
        result = steel_py.eval("(equal? payload (list 1 (hash \"ok\" #t) \"x\"))", bindings)
        self.assertTrue(result)

    def test_rational_round_trip(self) -> None:
        result = steel_py.eval("1/2")
        self.assertEqual(result, Fraction(1, 2))

        engine = steel_py.SteelEngine()
        engine.set("r", Fraction(2, 3))
        self.assertEqual(engine.eval("(+ r 1/3)"), Fraction(1, 1))

    def test_complex_round_trip(self) -> None:
        result = steel_py.eval("1+2i")
        self.assertEqual(result, complex(1.0, 2.0))

        engine = steel_py.SteelEngine()
        engine.set("z", complex(3, 4))
        self.assertEqual(engine.eval("(+ z 1)"), complex(4.0, 4.0))

    def test_mutable_vector_returns_python_list(self) -> None:
        self.assertEqual(steel_py.eval("(vector 10 20 30)"), [10, 20, 30])

    def test_hashset_returns_python_set(self) -> None:
        self.assertEqual(steel_py.eval("(hashset 10 20 30 30 40)"), {10, 20, 30, 40})

    def test_eval_surfaces_scheme_errors(self) -> None:
        with self.assertRaises(RuntimeError):
            steel_py.eval("(+ 1 'a)")


if __name__ == "__main__":
    unittest.main()
