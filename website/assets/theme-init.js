/* Applies the stored or system theme before the page paints. */
try{var t=localStorage.getItem("theme");document.documentElement.classList.toggle("dark",t==="dark"||(t!=="light"&&matchMedia("(prefers-color-scheme: dark)").matches))}catch(e){}
