/* Interface to Little CMS
   Copyright (C) 2017-2019 Free Software Foundation, Inc.

This file is part of GNU Emacs.

GNU Emacs is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or (at
your option) any later version.

GNU Emacs is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with GNU Emacs.  If not, see <https://www.gnu.org/licenses/>.  */

#include <config.h>

#ifdef HAVE_LCMS2

#include <lcms2.h>
#include <math.h>

#include "lisp.h"

typedef struct
{
  double J;
  double a;
  double b;
} lcmsJab_t;

#ifdef WINDOWSNT
# include <windows.h>
# include "w32.h"

DEF_DLL_FN (cmsFloat64Number, cmsCIE2000DeltaE,
	    (const cmsCIELab* Lab1, const cmsCIELab* Lab2, cmsFloat64Number Kl,
	     cmsFloat64Number Kc, cmsFloat64Number Kh));
DEF_DLL_FN (cmsHANDLE, cmsCIECAM02Init,
	    (cmsContext ContextID, const cmsViewingConditions* pVC));
DEF_DLL_FN (void, cmsCIECAM02Forward,
	    (cmsHANDLE hModel, const cmsCIEXYZ* pIn, cmsJCh* pOut));
DEF_DLL_FN (void, cmsCIECAM02Reverse,
	    (cmsHANDLE hModel, const cmsJCh* pIn, cmsCIEXYZ* pOut));
DEF_DLL_FN (void, cmsCIECAM02Done, (cmsHANDLE hModel));
DEF_DLL_FN (cmsBool, cmsWhitePointFromTemp,
	    (cmsCIExyY* WhitePoint, cmsFloat64Number TempK));
DEF_DLL_FN (void, cmsxyY2XYZ, (cmsCIEXYZ* Dest, const cmsCIExyY* Source));

static bool lcms_initialized;

static bool
init_lcms_functions (void)
{
  HMODULE library = w32_delayed_load (Qlcms2);

  if (!library)
    return false;

  LOAD_DLL_FN (library, cmsCIE2000DeltaE);
  LOAD_DLL_FN (library, cmsCIECAM02Init);
  LOAD_DLL_FN (library, cmsCIECAM02Forward);
  LOAD_DLL_FN (library, cmsCIECAM02Reverse);
  LOAD_DLL_FN (library, cmsCIECAM02Done);
  LOAD_DLL_FN (library, cmsWhitePointFromTemp);
  LOAD_DLL_FN (library, cmsxyY2XYZ);
  return true;
}

# undef cmsCIE2000DeltaE
# undef cmsCIECAM02Init
# undef cmsCIECAM02Forward
# undef cmsCIECAM02Reverse
# undef cmsCIECAM02Done
# undef cmsWhitePointFromTemp
# undef cmsxyY2XYZ

# define cmsCIE2000DeltaE      fn_cmsCIE2000DeltaE
# define cmsCIECAM02Init       fn_cmsCIECAM02Init
# define cmsCIECAM02Forward    fn_cmsCIECAM02Forward
# define cmsCIECAM02Reverse    fn_cmsCIECAM02Reverse
# define cmsCIECAM02Done       fn_cmsCIECAM02Done
# define cmsWhitePointFromTemp fn_cmsWhitePointFromTemp
# define cmsxyY2XYZ            fn_cmsxyY2XYZ

#endif	/* WINDOWSNT */

static bool
parse_lab_list (Lisp_Object lab_list, cmsCIELab *color)
{
#define PARSE_LAB_LIST_FIELD(field)					\
  if (CONSP (lab_list) && NUMBERP (XCAR (lab_list)))			\
    {									\
      color->field = XFLOATINT (XCAR (lab_list));			\
      lab_list = XCDR (lab_list);					\
    }									\
  else									\
    return false;

  PARSE_LAB_LIST_FIELD (L);
  PARSE_LAB_LIST_FIELD (a);
  PARSE_LAB_LIST_FIELD (b);

  return true;
}

/* http://www.ece.rochester.edu/~gsharma/ciede2000/ciede2000noteCRNA.pdf> */

DEFUN ("lcms-cie-de2000", Flcms_cie_de2000, Slcms_cie_de2000, 2, 5, 0,
       doc: /* Compute CIEDE2000 metric distance between COLOR1 and COLOR2.
Each color is a list of L*a*b* coordinates, where the L* channel ranges from
0 to 100, and the a* and b* channels range from -128 to 128.
Optional arguments KL, KC, KH are weighting parameters for lightness,
chroma, and hue, respectively. The parameters each default to 1.  */)
  (Lisp_Object color1, Lisp_Object color2,
   Lisp_Object kL, Lisp_Object kC, Lisp_Object kH)
{
  cmsCIELab Lab1, Lab2;
  cmsFloat64Number Kl, Kc, Kh;

#ifdef WINDOWSNT
  if (!lcms_initialized)
    lcms_initialized = init_lcms_functions ();
  if (!lcms_initialized)
    {
      message1 ("lcms2 library not found");
      return Qnil;
    }
#endif

  if (!(CONSP (color1) && parse_lab_list (color1, &Lab1)))
    signal_error ("Invalid color", color1);
  if (!(CONSP (color2) && parse_lab_list (color2, &Lab2)))
    signal_error ("Invalid color", color1);
  if (NILP (kL))
    Kl = 1.0f;
  else if (!(NUMBERP (kL) && (Kl = XFLOATINT(kL))))
    wrong_type_argument(Qnumberp, kL);
  if (NILP (kC))
    Kc = 1.0f;
  else if (!(NUMBERP (kC) && (Kc = XFLOATINT(kC))))
    wrong_type_argument(Qnumberp, kC);
  if (NILP (kL))
    Kh = 1.0f;
  else if (!(NUMBERP (kH) && (Kh = XFLOATINT(kH))))
    wrong_type_argument(Qnumberp, kH);

  return make_float (cmsCIE2000DeltaE (&Lab1, &Lab2, Kl, Kc, Kh));
}

static double
deg2rad (double degrees)
{
  return M_PI * degrees / 180.0;
}

static cmsCIEXYZ illuminant_d65 = { .X = 95.0455, .Y = 100.0, .Z = 108.8753 };

static void
default_viewing_conditions (const cmsCIEXYZ *wp, cmsViewingConditions *vc)
{
  vc->whitePoint.X = wp->X;
  vc->whitePoint.Y = wp->Y;
  vc->whitePoint.Z = wp->Z;
  vc->Yb = 20;
  vc->La = 100;
  vc->surround = AVG_SURROUND;
  vc->D_value = 1.0;
}

/* FIXME: code duplication */

static bool
parse_xyz_list (Lisp_Object xyz_list, cmsCIEXYZ *color)
{
#define PARSE_XYZ_LIST_FIELD(field)					\
  if (CONSP (xyz_list) && NUMBERP (XCAR (xyz_list)))			\
    {									\
      color->field = 100.0 * XFLOATINT (XCAR (xyz_list));		\
      xyz_list = XCDR (xyz_list);					\
    }									\
  else									\
    return false;

  PARSE_XYZ_LIST_FIELD (X);
  PARSE_XYZ_LIST_FIELD (Y);
  PARSE_XYZ_LIST_FIELD (Z);

  return true;
}

static bool
parse_viewing_conditions (Lisp_Object view, const cmsCIEXYZ *wp,
                          cmsViewingConditions *vc)
{
#define PARSE_VIEW_CONDITION_FLOAT(field)				\
  if (CONSP (view) && NUMBERP (XCAR (view)))				\
    {									\
      vc->field = XFLOATINT (XCAR (view));				\
      view = XCDR (view);						\
    }									\
  else									\
    return false;
#define PARSE_VIEW_CONDITION_INT(field)					\
  if (CONSP (view) && NATNUMP (XCAR (view)))				\
    {									\
      CHECK_RANGED_INTEGER (XCAR (view), 1, 4);				\
      vc->field = XINT (XCAR (view));					\
      view = XCDR (view);						\
    }									\
  else									\
    return false;

  PARSE_VIEW_CONDITION_FLOAT (Yb);
  PARSE_VIEW_CONDITION_FLOAT (La);
  PARSE_VIEW_CONDITION_INT (surround);
  PARSE_VIEW_CONDITION_FLOAT (D_value);

  if (! NILP (view))
    return false;

  vc->whitePoint.X = wp->X;
  vc->whitePoint.Y = wp->Y;
  vc->whitePoint.Z = wp->Z;
  return true;
}

/* References:
   Li, Luo et al. "The CRI-CAM02UCS colour rendering index." COLOR research
   and application, 37 No.3, 2012.
   Luo et al. "Uniform colour spaces based on CIECAM02 colour appearance
   model." COLOR research and application, 31 No.4, 2006. */

DEFUN ("lcms-cam02-ucs", Flcms_cam02_ucs, Slcms_cam02_ucs, 2, 4, 0,
       doc: /* Compute CAM02-UCS metric distance between COLOR1 and COLOR2.
Each color is a list of XYZ tristimulus values, with Y scaled about unity.
Optional argument WHITEPOINT is the XYZ white point, which defaults to
illuminant D65.
Optional argument VIEW is a list containing the viewing conditions, and
is of the form (YB LA SURROUND DVALUE) where SURROUND corresponds to
  1   AVG_SURROUND
  2   DIM_SURROUND
  3   DARK_SURROUND
  4   CUTSHEET_SURROUND
The default viewing conditions are (20 100 1 1).  */)
  (Lisp_Object color1, Lisp_Object color2, Lisp_Object whitepoint,
   Lisp_Object view)
{
  cmsViewingConditions vc;
  cmsJCh jch1, jch2;
  cmsCIEXYZ xyz1, xyz2, xyzw;
  lcmsJab_t jab1, jab2;
  double FL, k, k4;

#ifdef WINDOWSNT
  if (!lcms_initialized)
    lcms_initialized = init_lcms_functions ();
  if (!lcms_initialized)
    {
      message1 ("lcms2 library not found");
      return Qnil;
    }
#endif

  if (!(CONSP (color1) && parse_xyz_list (color1, &xyz1)))
    signal_error ("Invalid color", color1);
  if (!(CONSP (color2) && parse_xyz_list (color2, &xyz2)))
    signal_error ("Invalid color", color2);
  if (NILP (whitepoint))
    xyzw = illuminant_d65;
  else if (!(CONSP (whitepoint) && parse_xyz_list (whitepoint, &xyzw)))
    signal_error ("Invalid white point", whitepoint);
  if (NILP (view))
    default_viewing_conditions (&xyzw, &vc);
  else if (!(CONSP (view) && parse_viewing_conditions (view, &xyzw, &vc)))
    signal_error ("Invalid view conditions", view);

  h1 = cmsCIECAM02Init (0, &vc);
  h2 = cmsCIECAM02Init (0, &vc);
  cmsCIECAM02Forward (h1, &xyz1, &jch1);
  cmsCIECAM02Forward (h2, &xyz2, &jch2);
  cmsCIECAM02Done (h1);
  cmsCIECAM02Done (h2);

  /* Now have colors in JCh, need to calculate J'a'b'

     M = C * F_L^0.25
     J' = 1.7 J / (1 + 0.007 J)
     M' = 43.86 ln(1 + 0.0228 M)
     a' = M' cos(h)
     b' = M' sin(h)

     where

     F_L = 0.2 k^4 (5 L_A) + 0.1 (1 - k^4)^2 (5 L_A)^(1/3),
     k = 1/(5 L_A + 1)
  */
  k = 1.0 / (1.0 + (5.0 * vc.La));
  k4 = k * k * k * k;
  FL = vc.La * k4 + 0.1 * (1 - k4) * (1 - k4) * cbrt (5.0 * vc.La);
  Mp1 = 43.86 * log (1.0 + 0.0228 * (jch1.C * sqrt (sqrt (FL))));
  Mp2 = 43.86 * log (1.0 + 0.0228 * (jch2.C * sqrt (sqrt (FL))));
  Jp1 = 1.7 * jch1.J / (1.0 + (0.007 * jch1.J));
  Jp2 = 1.7 * jch2.J / (1.0 + (0.007 * jch2.J));
  ap1 = Mp1 * cos (deg2rad (jch1.h));
  ap2 = Mp2 * cos (deg2rad (jch2.h));
  bp1 = Mp1 * sin (deg2rad (jch1.h));
  bp2 = Mp2 * sin (deg2rad (jch2.h));

  return make_float (sqrt ((Jp2 - Jp1) * (Jp2 - Jp1) +
                           (ap2 - ap1) * (ap2 - ap1) +
                           (bp2 - bp1) * (bp2 - bp1)));
}

DEFUN ("lcms-temp->white-point", Flcms_temp_to_white_point, Slcms_temp_to_white_point, 1, 1, 0,
       doc: /* Return XYZ black body chromaticity from TEMPERATURE given in K.
Valid range of TEMPERATURE is from 4000K to 25000K.  */)
  (Lisp_Object temperature)
{
  cmsFloat64Number tempK;
  cmsCIExyY whitepoint;
  cmsCIEXYZ wp;

#ifdef WINDOWSNT
  if (!lcms_initialized)
    lcms_initialized = init_lcms_functions ();
  if (!lcms_initialized)
    {
      message1 ("lcms2 library not found");
      return Qnil;
    }
#endif

  CHECK_NUMBER_OR_FLOAT (temperature);

  tempK = XFLOATINT (temperature);
  if (!(cmsWhitePointFromTemp (&whitepoint, tempK)))
    signal_error("Invalid temperature", temperature);
  cmsxyY2XYZ (&wp, &whitepoint);
  return list3 (make_float (wp.X), make_float (wp.Y), make_float (wp.Z));
}

DEFUN ("lcms2-available-p", Flcms2_available_p, Slcms2_available_p, 0, 0, 0,
       doc: /* Return t if lcms2 color calculations are available in this instance of Emacs.  */)
     (void)
{
#ifdef WINDOWSNT
  Lisp_Object found = Fassq (Qlcms2, Vlibrary_cache);
  if (CONSP (found))
    return XCDR (found);
  else
    {
      Lisp_Object status;
      lcms_initialized = init_lcms_functions ();
      status = lcms_initialized ? Qt : Qnil;
      Vlibrary_cache = Fcons (Fcons (Qlcms2, status), Vlibrary_cache);
      return status;
    }
#else  /* !WINDOWSNT */
  return Qt;
#endif
}


/* Initialization */
void
syms_of_lcms2 (void)
{
  defsubr (&Slcms_cie_de2000);
  defsubr (&Slcms_xyz_to_jch);
  defsubr (&Slcms_jch_to_xyz);
  defsubr (&Slcms_jch_to_jab);
  defsubr (&Slcms_jab_to_jch);
  defsubr (&Slcms_cam02_ucs);
  defsubr (&Slcms2_available_p);
  defsubr (&Slcms_temp_to_white_point);

  Fprovide (intern_c_string ("lcms2"), Qnil);
}

#endif /* HAVE_LCMS2 */
