//! Extended system string calls operate on guest bytes, independently of UI encoding.
use crate::{Value, Vm, VmError, VmResult, native_utf8_probe, sentence_table};

impl Vm {
    fn raw_encoding_input(&self, value: Value, cp932_shadow: bool) -> VmResult<Vec<u8>> {
        let encode = |text: &str| {
            if cp932_shadow {
                encoding_rs::SHIFT_JIS.encode(text).0.into_owned()
            } else {
                self.encode_native_string(text).into_owned()
            }
        };
        let pointer = match value {
            Value::Str(text) => return Ok(encode(&text)),
            Value::Ptr(pointer) => pointer,
            Value::Int(pointer) => pointer as u32,
            Value::Func { offset, .. } => offset,
            Value::Program(_) | Value::None => return Ok(Vec::new()),
        };
        let bytes = self.read_c_string_bytes(pointer)?;
        // Shadow-only scenario records have no materialized bytes. Existing
        // guest bytes remain authoritative, even if their shadow decodes otherwise.
        if bytes.is_empty() {
            if let Some(Value::Str(text)) = self.mem_values.get(&self.value_key(pointer)) {
                return Ok(encode(text));
            }
        }
        Ok(bytes)
    }

    fn write_utf8_string(&mut self, destination: u32, text: String) -> VmResult<()> {
        let bytes = text.as_bytes();
        let range = self.resolve_write_range(destination, bytes.len() + 1)?;
        self.memory[range.start..range.start + bytes.len()].copy_from_slice(bytes);
        self.memory[range.start + bytes.len()] = 0;
        self.clear_shadow_values(destination, bytes.len() + 1);
        self.mem_values
            .insert(self.value_key(destination), Value::Str(text));
        Ok(())
    }

    pub(crate) fn sys81_convert_cp932(&mut self) -> VmResult<Value> {
        let source = self.pop_value()?;
        let destination = self.pop_ptr()?;
        let bytes = self.raw_encoding_input(source, true)?;
        let text = encoding_rs::SHIFT_JIS.decode(&bytes).0.into_owned();
        if destination != 0 {
            self.write_utf8_string(destination, text)?;
        }
        Ok(Value::None)
    }

    pub(crate) fn sys81_probe_utf8(&mut self) -> VmResult<Value> {
        let source = self.pop_value()?;
        let bytes = self.raw_encoding_input(source, false)?;
        Ok(Value::Int(native_utf8_probe::classify(&bytes)))
    }

    pub(crate) fn native_auto_string(&self, value: Value) -> VmResult<String> {
        let bytes = self.raw_encoding_input(value, false)?;
        Ok(if native_utf8_probe::classify(&bytes) == 0 {
            encoding_rs::SHIFT_JIS.decode(&bytes).0.into_owned()
        } else {
            String::from_utf8_lossy(&bytes).into_owned()
        })
    }

    pub(crate) fn sys81_sentence_lookup(&mut self) -> VmResult<Value> {
        let key = self.pop_value()?;
        let group = self.pop_value()?;
        let destination = self.pop_ptr()?;
        let key = self.native_auto_string(key)?;
        let group = self.native_auto_string(group)?;
        let replacement = self
            .system81_shared
            .lock()
            .map_err(|_| VmError::Runtime("sentence table state poisoned".into()))?
            .sentence_table
            .get(&group, &key)
            .map(str::to_owned);
        let Some(replacement) = replacement else {
            return Ok(Value::Int(0));
        };
        let length = replacement.len();
        if destination != 0 {
            self.write_utf8_string(destination, replacement)?;
        }
        Ok(Value::Int(length as i32))
    }

    pub(crate) fn sys81_sentence_import(&mut self) -> VmResult<Value> {
        let length = self.pop_int()?;
        let pointer = self.pop_ptr()?;
        if pointer == 0 {
            self.system81_shared
                .lock()
                .map_err(|_| VmError::Runtime("sentence table state poisoned".into()))?
                .sentence_table = sentence_table::SentenceTable::default();
            return Ok(Value::Int(1));
        }
        if length < 0 || !sentence_table::SentenceTable::valid_size(length as usize) {
            return Ok(Value::Int(0));
        }
        let bytes = self.resolve_range(pointer, length as usize)?;
        let Some(table) = sentence_table::SentenceTable::parse(&self.memory[bytes]) else {
            return Ok(Value::Int(0));
        };
        // Validate the complete bounded input before changing shared registrations.
        self.system81_shared
            .lock()
            .map_err(|_| VmError::Runtime("sentence table state poisoned".into()))?
            .sentence_table
            .merge(table);
        Ok(Value::Int(1))
    }
}
