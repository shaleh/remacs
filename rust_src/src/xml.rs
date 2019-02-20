//! Interface to libxml2.

use remacs_macros::lisp_fn;

use crate::{buffers::validate_region_rust, lisp::LispObject, remacs_sys::Qnil};

fn parse_region(start: EmacsInt, end: EmacsInt, base_url: LispObject, discard_comments: LispObject, is_html: bool) -> LispObject {
    validate_region_rust(&start, &end);

    let current_buffer = ThreadState::current_buffer_unchecked();
    let start_byte = buf_charpos_to_bytepos(current_buffer, start);
    let end_byte = CHAR_TO_BYTE (iend);

  if (istart < GPT && GPT < iend)
    move_gap_both (iend, iend_byte);

  if (! NILP (base_url))
    {
      CHECK_STRING (base_url);
      burl = SSDATA (base_url);
    }

  buftext = BYTE_POS_ADDR (istart_byte);
#ifdef REL_ALLOC
  /* Prevent ralloc.c from relocating the current buffer while libxml2
     functions below read its text.  */
  r_alloc_inhibit_buffer_relocation (1);
#endif
  if (htmlp)
    doc = htmlReadMemory ((char *)buftext,
			  iend_byte - istart_byte, burl, "utf-8",
			  HTML_PARSE_RECOVER|HTML_PARSE_NONET|
			  HTML_PARSE_NOWARNING|HTML_PARSE_NOERROR|
			  HTML_PARSE_NOBLANKS);
  else
    doc = xmlReadMemory ((char *)buftext,
			 iend_byte - istart_byte, burl, "utf-8",
			 XML_PARSE_NONET|XML_PARSE_NOWARNING|
			 XML_PARSE_NOBLANKS |XML_PARSE_NOERROR);

#ifdef REL_ALLOC
  r_alloc_inhibit_buffer_relocation (0);
#endif
  /* If the assertion below fails, malloc was called inside the above
     libxml2 functions, and ralloc.c caused relocation of buffer text,
     so we could have read from unrelated memory.  */
  eassert (buftext == BYTE_POS_ADDR (istart_byte));

  if (doc != NULL)
    {
      Lisp_Object r = Qnil;
      if (NILP(discard_comments))
        {
          /* If the document has toplevel comments, then this should
             get us the nodes and the comments. */
          xmlNode *n = doc->children;

          while (n) {
            if (!NILP (r))
              result = Fcons (r, result);
            r = make_dom (n);
            n = n->next;
          }
        }

      if (NILP (result)) {
	/* The document doesn't have toplevel comments or we discarded
	   them.  Get the tree the proper way. */
	xmlNode *node = xmlDocGetRootElement (doc);
	if (node != NULL)
	  result = make_dom (node);
      } else
	result = Fcons (Qtop, Fcons (Qnil, Fnreverse (Fcons (r, result))));

      xmlFreeDoc (doc);
    }

  return result;
}


#[cfg(feature = "use-xml2")]
use crate::remacs_sys::{init_libxml2_functions, parse_region};

#[cfg(feature = "use-xml2")]
fn libxml_parse_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
    htmlp: bool,
) -> LispObject {
    unsafe {
        if init_libxml2_functions() {
            parse_region(start, end, base_url, discard_comments, htmlp)
        } else {
            Qnil
        }
    }
}

#[cfg(not(feature = "use-xml2"))]
fn libxml_parse_region(
    _start: LispObject,
    _end: LispObject,
    _base_url: LispObject,
    _discard_comments: LispObject,
    _htmlp: bool,
) -> LispObject {
    Qnil
}

/// Parse the region as an HTML document and return the parse tree.
/// If BASE-URL is non-nil, it is used to expand relative URLs.
/// If DISCARD-COMMENTS is non-nil, all HTML comments are discarded.
#[lisp_fn(min = "2")]
pub fn libxml_parse_html_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
) -> LispObject {
    libxml_parse_region(start, end, base_url, discard_comments, true)
}

/// Parse the region as an XML document and return the parse tree.
/// If BASE-URL is non-nil, it is used to expand relative URLs.
/// If DISCARD-COMMENTS is non-nil, all HTML comments are discarded.
#[lisp_fn(min = "2")]
pub fn libxml_parse_xml_region(
    start: LispObject,
    end: LispObject,
    base_url: LispObject,
    discard_comments: LispObject,
) -> LispObject {
    libxml_parse_region(start, end, base_url, discard_comments, false)
}

/// Return t if libxml2 support is available in this instance of Emacs.
#[lisp_fn]
pub fn libxml_available_p() -> bool {
    cfg!(feature = "use-xml2")
}

include!(concat!(env!("OUT_DIR"), "/xml_exports.rs"));
