#[cfg(test)]
#[allow(clippy::module_inception)] // conventional inner test module
mod tests {
    use crate::stdlib;
    use crate::vm::Value;

    #[test]
    fn test_io_read_write() {
        let temp_file = "/tmp/nudge_test_io.txt";

        // Write
        let result = stdlib::io::execute(
            "io.write",
            vec![
                Value::String(temp_file.to_string()),
                Value::String("Hello, World!".to_string()),
            ],
        );
        assert!(result.is_ok());

        // Read
        let result = stdlib::io::execute("io.read", vec![Value::String(temp_file.to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::String("Hello, World!".to_string()));

        // Cleanup
        stdlib::io::execute("io.delete", vec![Value::String(temp_file.to_string())]).unwrap();
    }

    #[test]
    fn test_io_exists() {
        let result = stdlib::io::execute("io.exists", vec![Value::String("/tmp".to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Bool(true));

        let result =
            stdlib::io::execute("io.exists", vec![Value::String("/nonexistent".to_string())]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_math_abs() {
        let result = stdlib::math::execute("math.abs", vec![Value::Int(-5)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::math::execute("math.abs", vec![Value::Float(-3.15)]);
        assert_eq!(result.unwrap(), Value::Float(3.15));
    }

    #[test]
    fn test_math_min_max() {
        let result = stdlib::math::execute("math.min", vec![Value::Int(5), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::math::execute("math.max", vec![Value::Int(5), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(10));
    }

    #[test]
    fn test_math_sqrt() {
        let result = stdlib::math::execute("math.sqrt", vec![Value::Int(144)]);
        assert_eq!(result.unwrap(), Value::Float(12.0));
    }

    #[test]
    fn test_math_pow() {
        let result = stdlib::math::execute("math.pow", vec![Value::Int(2), Value::Int(10)]);
        assert_eq!(result.unwrap(), Value::Int(1024));
    }

    #[test]
    fn test_math_trig() {
        let result = stdlib::math::execute("math.sin", vec![Value::Float(0.0)]);
        assert_eq!(result.unwrap(), Value::Float(0.0));

        let result = stdlib::math::execute("math.cos", vec![Value::Float(0.0)]);
        assert_eq!(result.unwrap(), Value::Float(1.0));
    }

    #[test]
    fn test_math_mean() {
        let result = stdlib::math::execute(
            "math.mean",
            vec![Value::List(vec![
                Value::Int(1),
                Value::Int(2),
                Value::Int(3),
                Value::Int(4),
                Value::Int(5),
            ])],
        );
        assert_eq!(result.unwrap(), Value::Float(3.0));
    }

    #[test]
    fn test_str_length() {
        let result =
            stdlib::string::execute("str.length", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::Int(5));
    }

    #[test]
    fn test_str_upper_lower() {
        let result = stdlib::string::execute("str.upper", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::String("HELLO".to_string()));

        let result = stdlib::string::execute("str.lower", vec![Value::String("HELLO".to_string())]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));
    }

    #[test]
    fn test_str_trim() {
        let result =
            stdlib::string::execute("str.trim", vec![Value::String("  hello  ".to_string())]);
        assert_eq!(result.unwrap(), Value::String("hello".to_string()));
    }

    #[test]
    fn test_str_split_join() {
        let result = stdlib::string::execute(
            "str.split",
            vec![
                Value::String("a,b,c".to_string()),
                Value::String(",".to_string()),
            ],
        );
        assert_eq!(
            result.unwrap(),
            Value::List(vec![
                Value::String("a".to_string()),
                Value::String("b".to_string()),
                Value::String("c".to_string()),
            ])
        );

        let result = stdlib::string::execute(
            "str.join",
            vec![
                Value::List(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                    Value::String("c".to_string()),
                ]),
                Value::String("-".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::String("a-b-c".to_string()));
    }

    #[test]
    fn test_str_contains() {
        let result = stdlib::string::execute(
            "str.contains",
            vec![
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Bool(true));

        let result = stdlib::string::execute(
            "str.contains",
            vec![
                Value::String("hello world".to_string()),
                Value::String("xyz".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_str_replace() {
        let result = stdlib::string::execute(
            "str.replace",
            vec![
                Value::String("hello world".to_string()),
                Value::String("world".to_string()),
                Value::String("rust".to_string()),
            ],
        );
        assert_eq!(result.unwrap(), Value::String("hello rust".to_string()));
    }

    #[test]
    fn test_str_to_int() {
        let result = stdlib::string::execute("str.to_int", vec![Value::String("42".to_string())]);
        assert_eq!(result.unwrap(), Value::Int(42));

        let result = stdlib::string::execute("str.to_int", vec![Value::String("abc".to_string())]);
        assert_eq!(result.unwrap(), Value::None);
    }

    #[test]
    fn test_str_format() {
        let result = stdlib::string::execute(
            "str.format",
            vec![
                Value::String("Hello, {0}! You are {1} years old.".to_string()),
                Value::String("Alice".to_string()),
                Value::Int(25),
            ],
        );
        assert_eq!(
            result.unwrap(),
            Value::String("Hello, Alice! You are 25 years old.".to_string())
        );
    }

    #[test]
    fn test_stdlib_dispatch() {
        // Test that stdlib::execute routes correctly
        let result = stdlib::execute("math.abs", vec![Value::Int(-5)]);
        assert_eq!(result.unwrap(), Value::Int(5));

        let result = stdlib::execute("str.upper", vec![Value::String("hello".to_string())]);
        assert_eq!(result.unwrap(), Value::String("HELLO".to_string()));

        let result = stdlib::execute("io.exists", vec![Value::String("/tmp".to_string())]);
        assert_eq!(result.unwrap(), Value::Bool(true));
    }
}
