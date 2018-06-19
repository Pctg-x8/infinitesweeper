//! Combined Shader Decombiner

use bedrock as br;
use std::str::FromStr;
use std::mem::{align_of, size_of};
use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationOps { VertexInput, VertexShader, FragmentShader, Varyings }

pub struct Tokenizer<'s>(&'s str);
impl<'s> Tokenizer<'s> {
    pub fn new(s: &'s str) -> Self { Tokenizer(s) }

    fn strip_comment(&mut self) -> bool {
        if self.strip_prefix("//") {
            let strip_bytes = self.0.chars().take_while(|&c| c != '\n').fold(0, |a, c| a + c.len_utf8());
            self.0 = &self.0[strip_bytes..];
            return true;
        }
        else { false }
    }
    fn strip_ignores(&mut self) -> &mut Self {
        while self.strip_prefix(" ") || self.strip_prefix("\n") || self.strip_prefix("\r") || self.strip_prefix("\t") || self.strip_comment() { }
        return self;
    }

    fn strip_prefix(&mut self, p: &'s str) -> bool {
        if self.0.starts_with(p) { self.0 = &self.0[p.len()..]; return true; }
        else { false }
    }
    fn strip_ident(&mut self) -> Option<&'s str> {
        if self.0.starts_with(|c: char| c.is_digit(10)) { return None; }
        let bytes = self.0.chars().take_while(|&c| c.is_alphanumeric()).fold(0, |a, c| a + c.len_utf8());
        if bytes == 0 { return None; }
        let slice = &self.0[..bytes]; self.0 = &self.0[bytes..];
        return Some(slice);
    }
    pub fn ident_list(&mut self) -> Vec<&'s str> {
        self.strip_ignores();
        let v0 = if let Some(id) = self.strip_ident() { id } else { return Vec::new(); };
        let mut v = vec![v0];
        while self.strip_ignores().strip_prefix(",") {
            if let Some(id) = self.strip_ignores().strip_ident() { v.push(id); } else { break; }
        }
        return v;
    }

    pub fn glsl_type_ascription(&mut self) -> Option<&'s str> {
        self.strip_ignores();
        if !self.strip_prefix(":") { return None; }
        self.strip_ignores();
        let glsl_strip_bytes = self.0.chars().take_while(|&c| c != ';' && c != '@').fold(0, |a, c| a + c.len_utf8());
        if glsl_strip_bytes == 0 { return None; }
        let glsl_strip = &self.0[..glsl_strip_bytes];
        self.0 = &self.0[glsl_strip_bytes..];
        return Some(glsl_strip);
    }

    pub fn declaration_op(&mut self) -> Option<DeclarationOps> {
        self.strip_ignores();
        if self.strip_prefix("FragmentShader") { return Some(DeclarationOps::FragmentShader); }
        if self.strip_prefix("VertexShader") { return Some(DeclarationOps::VertexShader); }
        if self.strip_prefix("VertexInput") { return Some(DeclarationOps::VertexInput); }
        if self.strip_prefix("Varyings") { return Some(DeclarationOps::Varyings); }
        return None;
    }
    pub fn shader_stage(&mut self) -> Option<br::ShaderStage> {
        self.strip_ignores();
        if self.strip_prefix("FragmentShader") { return Some(br::ShaderStage::FRAGMENT); }
        if self.strip_prefix("VertexShader") { return Some(br::ShaderStage::VERTEX); }
        return None;
    }
    pub fn codeblock(&mut self) -> Option<&'s str> {
        self.strip_ignores();
        if !self.block_start() { return None; }
        fn strip_bytes_counter<I: Iterator<Item = char>>(mut c: I, current: usize, nestlevel: usize) -> Option<usize> {
            match c.next() {
                Some(cc @ '{') => strip_bytes_counter(c, current + cc.len_utf8(), nestlevel + 1),
                Some(cc @ '}') => if nestlevel == 0 { Some(current) }
                    else { strip_bytes_counter(c, current + cc.len_utf8(), nestlevel - 1) },
                Some(cc) => strip_bytes_counter(c, current + cc.len_utf8(), nestlevel),
                None => None
            }
        }
        let cb_slice_bytes = strip_bytes_counter(self.0.chars(), 0, 0).expect("Missing closing brace");
        let cb_slice = &self.0[..cb_slice_bytes];
        self.0 = &self.0[cb_slice_bytes + 1..];
        return Some(cb_slice);
    }
    pub fn binding(&mut self) -> Option<(usize, br::vk::VkVertexInputRate)> {
        self.strip_ignores();
        if !self.strip_prefix("Binding") { return None; }
        let index = if let Some(n) = self.index_number() { n } else { return None; };
        let irate = if self.bracket_start() {
            self.strip_ignores();
            let r =
                if self.strip_prefix("PerInstance") { br::vk::VK_VERTEX_INPUT_RATE_INSTANCE }
                else if self.strip_prefix("PerVertex") { br::vk::VK_VERTEX_INPUT_RATE_VERTEX }
                else { return None };
            if !self.bracket_end() { return None; } else { r }
        }
        else { br::vk::VK_VERTEX_INPUT_RATE_VERTEX };
        return (index, irate).into();
    }

    pub fn index_number(&mut self) -> Option<usize> {
        self.strip_ignores();
        let num_bytes = self.0.chars().take_while(|&c| c.is_digit(10)).fold(0, |a, c| a + c.len_utf8());
        if let Ok(n) = usize::from_str(&self.0[..num_bytes]) {
            self.0 = &self.0[num_bytes..]; return Some(n);
        }
        else { None }
    }

    pub fn block_start(&mut self) -> bool { self.strip_ignores(); self.strip_prefix("{") }
    pub fn block_end(&mut self) -> bool { self.strip_ignores(); self.strip_prefix("}") }
    pub fn bracket_start(&mut self) -> bool { self.strip_ignores(); self.strip_prefix("[") }
    pub fn bracket_end(&mut self) -> bool { self.strip_ignores(); self.strip_prefix("]") }
    pub fn declaration_end(&mut self) -> bool { self.strip_ignores().strip_prefix(";") }
    pub fn arrow(&mut self) -> bool { self.strip_ignores().strip_prefix("->") }

    pub fn no_chars(&self) -> bool { self.0.is_empty() }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Variable<'s> { name: &'s str, type_str: &'s str }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingBlock<'s> {
    rate: br::vk::VkVertexInputRate, vars: Vec<Variable<'s>>
}
impl<'s> BindingBlock<'s> {
    fn packed_size(&self) -> usize {
        let mut total = 0;
        for &Variable { type_str, .. } in &self.vars {
            if type_str == "vec4" {
                let offs = align2(total, align_of::<[f32; 4]>());
                total = offs + size_of::<[f32; 4]>();
            }
            else if type_str == "vec2" {
                let offs = align2(total, align_of::<[f32; 2]>());
                total = offs + size_of::<[f32; 2]>();
            }
            else if type_str == "float" {
                let offs = align2(total, align_of::<f32>());
                total = offs + size_of::<f32>();
            }
            else { println!("Warning: Unable to determine exact packed size"); }
        }
        return total;
    }
}

fn align2(x: usize, a: usize) -> usize { (x + (a - 1)) & !(a - 1) }

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToplevelBlock<'s> {
    VertexInput(Vec<(usize, BindingBlock<'s>)>),
    ShaderCode(br::ShaderStage, &'s str),
    Varying(br::ShaderStage, br::ShaderStage, Vec<Variable<'s>>)
}
impl<'s> Tokenizer<'s> {
    pub fn binding_block(&mut self) -> Option<(usize, BindingBlock<'s>)> {
        let (index, rate) = if let Some(v) = self.binding() { v } else { return None; };
        if !self.block_start() { return None; }
        let mut vars = Vec::new();
        while !self.block_end() {
            let idents = self.ident_list();
            if idents.is_empty() { panic!("No declaration found"); }
            let glsl_type_str = self.glsl_type_ascription().expect("Type required");
            vars.extend(idents.into_iter().map(|name| Variable { name, type_str: glsl_type_str }));
            if !self.declaration_end() { panic!("; required at end of each declaration"); }
        }
        return Some((index, BindingBlock { rate, vars }));
    }
    pub fn vertex_input_block(&mut self) -> Vec<(usize, BindingBlock<'s>)> {
        if !self.block_start() { return Vec::new(); }
        let mut bindings = Vec::new();
        while !self.block_end() {
            bindings.push(self.binding_block().expect("Binding block required"));
        }
        return bindings;
    }

    pub fn varying(&mut self) -> (br::ShaderStage, br::ShaderStage, Vec<Variable<'s>>) {
        let src = self.shader_stage().expect("Source ShaderStage required");
        if !self.arrow() { panic!("Arrow required"); }
        let dst = self.shader_stage().expect("Destination ShaderStage required");
        if !self.block_start() { panic!("Variable Block required"); }
        let mut vars = Vec::new();
        while !self.block_end() {
            let idents = self.ident_list();
            if idents.is_empty() { panic!("No declaration found"); }
            let glsl_type_str = self.glsl_type_ascription().expect("Type required");
            vars.extend(idents.into_iter().map(|name| Variable { name, type_str: glsl_type_str }));
            if !self.declaration_end() { panic!("; required at end of each declaration"); }
        }
        return (src, dst, vars);
    }

    pub fn toplevel_block(&mut self) -> Option<ToplevelBlock<'s>> {
        match self.declaration_op() {
            Some(DeclarationOps::VertexInput) => {
                let vi = self.vertex_input_block();
                if vi.is_empty() { None } else { ToplevelBlock::VertexInput(vi).into() }
            },
            Some(DeclarationOps::VertexShader) =>
                self.codeblock().map(|c| ToplevelBlock::ShaderCode(br::ShaderStage::VERTEX, c)),
            Some(DeclarationOps::FragmentShader) =>
                self.codeblock().map(|c| ToplevelBlock::ShaderCode(br::ShaderStage::FRAGMENT, c)),
            Some(DeclarationOps::Varyings) => {
                let (src, dst, vars) = self.varying();
                return ToplevelBlock::Varying(src, dst, vars).into();
            },
            _ => None
        }
    }

    pub fn toplevel_blocks(&mut self) -> Vec<ToplevelBlock<'s>> {
        let mut blocks = Vec::new();
        while !self.strip_ignores().no_chars() {
            blocks.push(self.toplevel_block().expect("Toplevel-Block required"));
        }
        return blocks;
    }
}

#[derive(Debug, Clone)]
pub struct CombinedShader<'s> {
    vertex_input: Vec<(usize, BindingBlock<'s>)>,
    vertex_shader_code: &'s str, fragment_shader_code: Option<&'s str>,
    varyings_between_shaders: Vec<(br::ShaderStage, br::ShaderStage, Vec<Variable<'s>>)>
}
impl<'s> CombinedShader<'s> {
    pub fn from_parsed_blocks(blocks: Vec<ToplevelBlock<'s>>) -> Self {
        let mut cs = CombinedShader {
            vertex_input: Vec::new(),
            vertex_shader_code: "", fragment_shader_code: None,
            varyings_between_shaders: Vec::new()
        };

        for tb in blocks {
            match tb {
                ToplevelBlock::VertexInput(mut bindings) => cs.vertex_input.append(&mut bindings),
                ToplevelBlock::ShaderCode(br::ShaderStage::VERTEX, c) => {
                    if cs.vertex_shader_code.is_empty() { cs.vertex_shader_code = c; }
                    else { panic!("Multiple Vertex Shader code"); }
                },
                ToplevelBlock::ShaderCode(br::ShaderStage::FRAGMENT, c) => {
                    if cs.fragment_shader_code.is_none() { cs.fragment_shader_code = Some(c); }
                    else { panic!("Multiple Fragment Shader code"); }
                },
                ToplevelBlock::ShaderCode(ss, _) => panic!("Unsupported Shader Stage: {:08b}", ss.0),
                ToplevelBlock::Varying(src, dst, vars) => cs.varyings_between_shaders.push((src, dst, vars)),
            }
        }
        if cs.vertex_shader_code.is_empty() { panic!("VertexShader is not specified"); }
        return cs;
    }

    pub fn is_provided_fsh(&self) -> bool { self.fragment_shader_code.is_some() }

    pub fn emit_vertex_shader(&self) -> String {
        let mut code = String::from("#version 450\n\n");

        // 入力変数(vertex_inputから)
        for (n, vi_vars) in self.vertex_input.iter().flat_map(|&(_, ref bb)| &bb.vars).enumerate() {
            code += &format!("layout(location = {}) in {} {};\n", n, vi_vars.type_str, vi_vars.name);
        }
        // 出力変数
        for (n, ovar) in self.varyings_between_shaders.iter()
            .filter(|&&(src, _, _)| src == br::ShaderStage::VERTEX)
            .flat_map(|&(_, _, ref v)| v).enumerate() {
            code += &format!("layout(location = {}) out {} {};\n", n, ovar.type_str, ovar.name);
        }
        // gl_Positionの宣言を追加
        if self.vertex_shader_code.contains("RasterPosition") {
            code += "out gl_PerVertex { out vec4 gl_Position; };\n";
        }
        code += "\n";
        // main
        code += &format!("void main() {{{}}}", self.vertex_shader_code.replace("RasterPosition", "gl_Position"));
        return code;
    }
    pub fn emit_fragment_shader(&self) -> String {
        let mut code = String::from("#version 450\n\n");

        // 入力変数(varyingsから)
        for (n, ivar) in self.varyings_between_shaders.iter()
            .filter(|&&(_, dst, _)| dst == br::ShaderStage::FRAGMENT)
            .flat_map(|&(_, _, ref v)| v).enumerate() {
            code += &format!("layout(location = {}) in {} {};\n", n, ivar.type_str, ivar.name);
        }
        // 出力変数(ソースコード中/Target\[\d+\]/から)
        let mut fragment_code = String::from(*self.fragment_shader_code.as_ref().expect("No fragment shader"));
        let rx = Regex::new(r"Target\[(\d+)\]").unwrap();
        loop {
            let replace_index = if let Some(caps) = rx.captures(&fragment_code) {
                let index = caps.get(1).unwrap();
                code += &format!("layout(location = {index}) out vec4 sv_target_{index};\n", index = index.as_str());
                usize::from_str(index.as_str()).unwrap()
            }
            else { break; };
            fragment_code = fragment_code.replace(&format!("Target[{}]", replace_index), &format!("sv_target_{}", replace_index));
        }
        code += "\n";
        // main
        code += &format!("void main() {{{}}}", fragment_code);
        return code;
    }
    pub fn emit_vertex_bindings(&self) -> Vec<br::vk::VkVertexInputBindingDescription> {
        self.vertex_input.iter().map(|&(binding, ref blk)| br::vk::VkVertexInputBindingDescription {
            binding: binding as _, inputRate: blk.rate, stride: blk.packed_size() as _
        }).collect()
    }
    pub fn emit_vertex_attributes(&self) -> Vec<br::vk::VkVertexInputAttributeDescription> {
        let mut attrs = Vec::new();
        let mut location_offs = 0;
        for &(binding, ref blk) in self.vertex_input.iter() {
            let mut offs_in_binding = 0;
            for (loc_offs, &Variable { type_str, .. }) in blk.vars.iter().enumerate() {
                match type_str {
                    "vec4" => {
                        attrs.push(br::vk::VkVertexInputAttributeDescription {
                            location: (location_offs + loc_offs) as _,
                            binding: binding as _, format: br::vk::VK_FORMAT_R32G32B32A32_SFLOAT,
                            offset: offs_in_binding as _
                        });
                        offs_in_binding = align2(offs_in_binding + size_of::<[f32; 4]>(), align_of::<[f32; 4]>());
                    },
                    "vec2" => {
                        attrs.push(br::vk::VkVertexInputAttributeDescription {
                            location: (location_offs + loc_offs) as _,
                            binding: binding as _, format: br::vk::VK_FORMAT_R32G32_SFLOAT,
                            offset: offs_in_binding as _
                        });
                        offs_in_binding = align2(offs_in_binding + size_of::<[f32; 2]>(), align_of::<[f32; 2]>());
                    },
                    "float" => {
                        attrs.push(br::vk::VkVertexInputAttributeDescription {
                            location: (location_offs + loc_offs) as _,
                            binding: binding as _, format: br::vk::VK_FORMAT_R32_SFLOAT,
                            offset: offs_in_binding as _
                        });
                        offs_in_binding = align2(offs_in_binding + size_of::<f32>(), align_of::<f32>());
                    },
                    _ => println!("Warning: Cannot estimate appropriate attribute info")
                }
            }
            location_offs += blk.vars.len();
        }
        return attrs;
    }
}
