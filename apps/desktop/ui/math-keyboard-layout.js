import {referenceKeys} from './math-keyboard-data.js';
const original=s=>referenceKeys.find(k=>k.s===s);
// Keep the supplied notation data; computational keys add complete operands/binders.
export const computationalLatexGroups={
 basic:[...['7','8','9','4','5','6','1','2','3','0','.','x','y','+','-','=','/'].map(c=>({s:c,c})),{s:'×',c:'\\cdot '},original('a⁄b'),original('aᵇ'),original('√x'),original('|x|'),original('( )'),{s:'π',c:'\\pi '},{s:'x₁',c:'_{}'}],
 functions:['sin','cos','exp','ln'].map(s=>({s,c:`\\${s}\\left(□\\right)`})),
 calculus:[{...original('∫'),c:'\\int_{0}^{1} {□}\\,dx'},{s:'∫ dx',c:'\\int {□}\\,dx'},{...original('d/dx'),c:'\\frac{d}{dx}\\left(□\\right)'},{...original('∂/∂x'),c:'\\frac{\\partial}{\\partial x}\\left(□\\right)'},{s:'Σ',c:'\\sum_{i=1}^{□}{□}'},{s:'Π',c:'\\prod_{i=1}^{□}{□}'}],
 matrices:[{s:'2×2',c:'\\begin{bmatrix}□ & □ \\\\ □ & □\\end{bmatrix}'},{s:'3×3',c:'\\begin{bmatrix}□ & □ & □ \\\\ □ & □ & □ \\\\ □ & □ & □\\end{bmatrix}'},{s:'2×1',c:'\\begin{bmatrix}□ \\\\ □\\end{bmatrix}'},{s:'Aᵀ',c:'^{T}'},{s:'A⁻¹',c:'^{-1}'},{s:'det',c:'\\det\\left(□\\right)'},{s:'=',c:'='}],
 greek:['alpha','beta','gamma','delta','theta','lambda','mu','sigma','omega'].map((c,i)=>({s:['α','β','γ','δ','θ','λ','μ','σ','ω'][i],c:`\\${c} `}))
};
export const referenceNotation=[...new Map(referenceKeys.filter(k=>k.c&&k.c.trim()).map(k=>[k.c,k])).values()];
