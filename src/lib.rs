#![allow(unsafe_op_in_unsafe_fn)]

use pyo3::prelude::*;
use pyo3::types::{PyAny, PyComplex, PyDict, PyList, PySet, PyTuple};
use std::collections::HashMap;
use std::env;
use std::fs;
use std::str::FromStr;
use steel::SteelVal;
use steel::rvals::{IntoSteelVal, SteelComplex};
use steel::steel_vm::engine::Engine;

use num_bigint::BigInt;
use num_rational::{BigRational, Rational32};

fn ensure_steel_home() -> PyResult<()> {
    if env::var_os("STEEL_HOME").is_some() {
        return Ok(());
    }

    let path = env::temp_dir().join("steel_py_home");
    fs::create_dir_all(&path).map_err(|e| py_err(e.to_string()))?;
    // Set once during module use so Steel can resolve its runtime home.
    unsafe { env::set_var("STEEL_HOME", path) };
    Ok(())
}

fn py_err(message: String) -> PyErr {
    pyo3::exceptions::PyRuntimeError::new_err(message)
}

#[pyfunction]
#[pyo3(signature = (code, bindings=None))]
fn eval(py: Python<'_>, code: &str, bindings: Option<&Bound<'_, PyDict>>) -> PyResult<PyObject> {
    ensure_steel_home()?;
    let mut engine = Engine::new();
    apply_bindings(&mut engine, bindings)?;
    eval_with_engine(py, &mut engine, code)
}

#[pyclass(unsendable)]
struct SteelEngine {
    engine: Engine,
}

#[pymethods]
impl SteelEngine {
    #[new]
    fn new() -> PyResult<Self> {
        ensure_steel_home()?;
        Ok(Self {
            engine: Engine::new(),
        })
    }

    #[pyo3(signature = (code, bindings=None))]
    fn eval(
        &mut self,
        py: Python<'_>,
        code: &str,
        bindings: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        apply_bindings(&mut self.engine, bindings)?;
        eval_with_engine(py, &mut self.engine, code)
    }

    fn set(&mut self, name: &str, value: &Bound<'_, PyAny>) -> PyResult<()> {
        let steel_value = py_to_steel(value)?;
        self.engine.register_value(name, steel_value);
        Ok(())
    }

    #[pyo3(signature = (name, *args))]
    fn call(
        &mut self,
        py: Python<'_>,
        name: &str,
        args: &Bound<'_, PyTuple>,
    ) -> PyResult<PyObject> {
        let mut converted = Vec::with_capacity(args.len());
        for item in args.iter() {
            converted.push(py_to_steel(&item)?);
        }

        let result = self
            .engine
            .call_function_by_name_with_args(name, converted)
            .map_err(|e| py_err(e.to_string()))?;
        steel_to_python(py, result)
    }
}

fn eval_with_engine(py: Python<'_>, engine: &mut Engine, code: &str) -> PyResult<PyObject> {
    let values = engine
        .run(code.to_owned())
        .map_err(|e| py_err(e.to_string()))?;
    let last = values.last().cloned().unwrap_or(SteelVal::Void);
    steel_to_python(py, last)
}

fn apply_bindings(engine: &mut Engine, bindings: Option<&Bound<'_, PyDict>>) -> PyResult<()> {
    let Some(bindings) = bindings else {
        return Ok(());
    };

    for (key, value) in bindings.iter() {
        let name = match key.extract::<String>() {
            Ok(name) => name,
            Err(_) => key.str()?.to_str()?.to_owned(),
        };
        let steel_value = py_to_steel(&value)?;
        engine.register_value(&name, steel_value);
    }

    Ok(())
}

fn py_to_steel(value: &Bound<'_, PyAny>) -> PyResult<SteelVal> {
    if value.is_none() {
        return ().into_steelval().map_err(|e| py_err(e.to_string()));
    }

    if is_fraction_instance(value)? {
        return py_fraction_to_steel(value);
    }
    if let Ok(v) = value.downcast::<PyComplex>() {
        let complex = SteelComplex::new(SteelVal::NumV(v.real()), SteelVal::NumV(v.imag()));
        return complex.into_steelval().map_err(|e| py_err(e.to_string()));
    }

    if let Ok(v) = value.extract::<bool>() {
        return v.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.extract::<i64>() {
        return v.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.extract::<f64>() {
        return v.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.extract::<String>() {
        return v.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.downcast::<PyList>() {
        let mut out = Vec::with_capacity(v.len());
        for item in v.iter() {
            out.push(py_to_steel(&item)?);
        }
        return out.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.downcast::<PyTuple>() {
        let mut out = Vec::with_capacity(v.len());
        for item in v.iter() {
            out.push(py_to_steel(&item)?);
        }
        return out.into_steelval().map_err(|e| py_err(e.to_string()));
    }
    if let Ok(v) = value.downcast::<PyDict>() {
        let mut out: HashMap<SteelVal, SteelVal> = HashMap::new();
        for (k, val) in v.iter() {
            out.insert(py_to_steel(&k)?, py_to_steel(&val)?);
        }
        return out.into_steelval().map_err(|e| py_err(e.to_string()));
    }

    Err(py_err(format!(
        "Unsupported Python type for Steel conversion: {}",
        value.get_type().name()?
    )))
}

fn is_fraction_instance(value: &Bound<'_, PyAny>) -> PyResult<bool> {
    let py = value.py();
    let fractions = py.import_bound("fractions")?;
    let fraction_type = fractions.getattr("Fraction")?;
    value.is_instance(&fraction_type)
}

fn py_fraction_to_steel(value: &Bound<'_, PyAny>) -> PyResult<SteelVal> {
    let numer_obj = value.getattr("numerator")?;
    let denom_obj = value.getattr("denominator")?;

    if let (Ok(n), Ok(d)) = (numer_obj.extract::<i32>(), denom_obj.extract::<i32>()) {
        return Rational32::new(n, d)
            .into_steelval()
            .map_err(|e| py_err(e.to_string()));
    }

    let numer_str = numer_obj.str()?.to_str()?.to_owned();
    let denom_str = denom_obj.str()?.to_str()?.to_owned();
    let numer = BigInt::from_str(&numer_str).map_err(|e| py_err(e.to_string()))?;
    let denom = BigInt::from_str(&denom_str).map_err(|e| py_err(e.to_string()))?;
    BigRational::new(numer, denom)
        .into_steelval()
        .map_err(|e| py_err(e.to_string()))
}

fn python_fraction_from_parts(py: Python<'_>, numer: &str, denom: &str) -> PyResult<PyObject> {
    let fractions = py.import_bound("fractions")?;
    let fraction_type = fractions.getattr("Fraction")?;
    let builtins = py.import_bound("builtins")?;
    let int_type = builtins.getattr("int")?;
    let py_numer = int_type.call1((numer,))?;
    let py_denom = int_type.call1((denom,))?;
    Ok(fraction_type.call1((py_numer, py_denom))?.into_py(py))
}

fn steel_number_to_f64(value: &SteelVal) -> Option<f64> {
    match value {
        SteelVal::IntV(v) => Some(*v as f64),
        SteelVal::NumV(v) => Some(*v),
        SteelVal::Rational(v) => Some(*v.numer() as f64 / *v.denom() as f64),
        SteelVal::BigNum(v) => v.to_string().parse::<f64>().ok(),
        SteelVal::BigRational(v) => {
            let n = v.numer().to_string().parse::<f64>().ok()?;
            let d = v.denom().to_string().parse::<f64>().ok()?;
            Some(n / d)
        }
        _ => None,
    }
}
// TODO: maybe distinguish strings and symbols
// eval_from_file

fn steel_to_python(py: Python<'_>, value: SteelVal) -> PyResult<PyObject> {
    match value {
        SteelVal::BoolV(v) => Ok(v.into_py(py)),
        SteelVal::IntV(v) => Ok(v.into_py(py)),
        SteelVal::NumV(v) => Ok(v.into_py(py)),
        SteelVal::Rational(v) => {
            python_fraction_from_parts(py, &v.numer().to_string(), &v.denom().to_string())
        }
        SteelVal::BigRational(v) => {
            python_fraction_from_parts(py, &v.numer().to_string(), &v.denom().to_string())
        }
        SteelVal::Complex(v) => {
            let re = steel_number_to_f64(&v.re)
                .ok_or_else(|| py_err("Unable to convert complex real part".to_owned()))?;
            let im = steel_number_to_f64(&v.im)
                .ok_or_else(|| py_err("Unable to convert complex imaginary part".to_owned()))?;
            Ok(PyComplex::from_doubles_bound(py, re, im)
                .into_any()
                .unbind()
                .into())
        }
        SteelVal::StringV(v) => Ok(v.to_string().into_py(py)),
        SteelVal::SymbolV(v) => Ok(v.to_string().into_py(py)),
        SteelVal::CharV(v) => Ok(v.to_string().into_py(py)),
        SteelVal::Void => Ok(py.None()),
        SteelVal::VectorV(v) => {
            let list = PyList::empty_bound(py);
            for item in v.iter() {
                list.append(steel_to_python(py, item.clone())?)?;
            }
            Ok(list.into_any().unbind())
        }
        SteelVal::ListV(v) => {
            let list = PyList::empty_bound(py);
            for item in v.iter() {
                list.append(steel_to_python(py, item.clone())?)?;
            }
            Ok(list.into_any().unbind())
        }
        SteelVal::MutableVector(v) => {
            let list = PyList::empty_bound(py);
            for item in v.get() {
                list.append(steel_to_python(py, item)?)?;
            }
            Ok(list.into_any().unbind())
        }
        SteelVal::HashMapV(v) => {
            let dict = PyDict::new_bound(py);
            for (key, val) in v.iter() {
                let py_val = steel_to_python(py, val.clone())?;
                match dict.set_item(steel_to_python(py, key.clone())?, &py_val) {
                    Ok(()) => {}
                    Err(_) => {
                        dict.set_item(key.to_string(), &py_val)?;
                    }
                }
            }
            Ok(dict.into_any().unbind())
        }
        SteelVal::HashSetV(v) => {
            let set = PySet::empty_bound(py)?;
            for item in v.iter() {
                set.add(steel_to_python(py, item.clone())?)?;
            }
            Ok(set.into_any().unbind())
        }
        SteelVal::Pair(pair) => {
            let car = steel_to_python(py, pair.car())?;
            let cdr = steel_to_python(py, pair.cdr())?;
            Ok((car, cdr).into_py(py))
        }
        other => Ok(other.to_string().into_py(py)),
    }
}

#[pymodule]
fn steel_py(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<SteelEngine>()?;
    m.add_function(wrap_pyfunction!(eval, m)?)?;
    Ok(())
}
