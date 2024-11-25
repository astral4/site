//! Code for building complete HTML pages from article bodies.

use crate::OUTPUT_SITE_CSS_FILE;
use anyhow::{Error, Result};
use ego_tree::{tree, Tree};
use markup5ever::{namespace_url, ns, Attribute, LocalName, QualName};
use scraper::{
    node::{Doctype, Element, Node, Text},
    Html,
};

pub struct PageBuilder {
    body: Tree<Node>,
}

impl PageBuilder {
    /// Initializes the webpage HTML builder, parsing an input string as a HTML `<body>`.
    ///
    /// # Errors
    /// This function returns an error if the input string could not be successfully parsed as no-quirks HTML.
    pub fn new(body: &str) -> Result<Self> {
        let body = Html::parse_fragment(body);

        match body.errors.first() {
            Some(err) => Err(Error::msg(err.clone())
                .context("encountered errors when parsing page body as HTML")),
            None => Ok(Self { body: body.tree }),
        }
    }

    /// Consumes the webpage HTML builder, outputting a string containing a complete HTML document.
    /// The parameters determine the contents of various metadata tags in the HTML `<head>` element.
    #[must_use]
    pub fn build_page(self, title: &str, author: &str) -> String {
        let mut html = Html::new_document();
        let mut root = html.tree.root_mut();

        root.append(Node::Doctype(Doctype {
            name: "html".into(),
            public_id: "".into(),
            system_id: "".into(),
        }));

        let html_element = create_el_with_attrs("html", vec![("lang", "en")]);

        let mut html_element_node = root.append(html_element);

        html_element_node.append_subtree(tree! {
            create_el("head") => {
                create_el_with_attrs("meta", vec![("charset", "utf-8")]),
                create_el_with_attrs("meta", vec![("name", "viewport"), ("content", "width=device-width, initial-scale=1")]),
                create_el_with_attrs("meta", vec![("name", "author"), ("content", author)]),
                create_el("title") => { create_text(title) },
                create_el_with_attrs("link", vec![("rel", "stylesheet"), ("href", OUTPUT_SITE_CSS_FILE)])
            }
        });

        let mut body_element_node = html_element_node.append(create_el("body"));

        body_element_node.append_subtree(self.body);

        html.html()
    }
}

fn create_el(name: &str) -> Node {
    Node::Element(Element::new(create_name(name), vec![]))
}

fn create_el_with_attrs<'a, I>(name: &str, attrs: I) -> Node
where
    I: IntoIterator,
    I::IntoIter: Iterator<Item = (&'a str, &'a str)>,
{
    let attrs = attrs
        .into_iter()
        .map(|(key, value)| Attribute {
            name: create_name(key),
            value: value.into(),
        })
        .collect();

    Node::Element(Element::new(create_name(name), attrs))
}

fn create_name(name: &str) -> QualName {
    QualName {
        prefix: None,
        ns: ns!(html),
        local: LocalName::try_static(name)
            .expect("calls to this function should supply valid names"),
    }
}

fn create_text(text: &str) -> Node {
    Node::Text(Text { text: text.into() })
}
